// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Linux game-session lifecycle, independent of displays and UI toolkits.
//! This crate deliberately contains no graphics compositor.
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone)]
pub struct GameConfig {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub running_pid: Option<u32>,
    pub overlay_open: bool,
    pub last_exit: Option<i32>,
}

impl Snapshot {
    pub fn to_wire(&self) -> String {
        let mode = if self.running_pid.is_some() { "game" } else { "home" };
        let pid = self.running_pid.map(|p| p.to_string()).unwrap_or_else(|| "none".into());
        let exit = self.last_exit.map(|e| e.to_string()).unwrap_or_else(|| "none".into());
        format!("mode={mode} overlay={} pid={pid} last_exit={exit}\n", u8::from(self.overlay_open))
    }
}

/// Owns only children it spawned. Commands never accept arbitrary shell strings
/// or operating-system PIDs. A future compositor can watch the same state.
pub struct Supervisor {
    game_config: Option<GameConfig>,
    game: Option<Child>,
    overlay_open: bool,
    last_exit: Option<i32>,
}

impl Supervisor {
    pub fn new(game_config: Option<GameConfig>) -> Self {
        Self { game_config, game: None, overlay_open: false, last_exit: None }
    }

    /// Reap completed children before reporting state.
    pub fn snapshot(&mut self) -> io::Result<Snapshot> {
        let finished = match self.game.as_mut() {
            Some(child) => child.try_wait()?,
            None => None,
        };
        if let Some(status) = finished {
            self.game.take();
            self.overlay_open = false;
            self.last_exit = status.code();
        }
        Ok(Snapshot {
            running_pid: self.game.as_ref().map(Child::id),
            overlay_open: self.overlay_open,
            last_exit: self.last_exit,
        })
    }

    pub fn start(&mut self) -> io::Result<u32> {
        if let Some(pid) = self.snapshot()?.running_pid {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("game already running: {pid}")));
        }
        let config = self.game_config.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "no native game configured (--game PATH)")
        })?;
        let child = Command::new(&config.program)
            .args(&config.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;
        let pid = child.id();
        self.game = Some(child);
        self.overlay_open = false;
        self.last_exit = None;
        Ok(pid)
    }

    pub fn stop(&mut self) -> io::Result<()> {
        if let Some(mut child) = self.game.take() {
            // The v0.1 implementation uses Child::kill (SIGKILL on Linux).
            // A SIGTERM -> timeout -> SIGKILL policy belongs in the next iteration.
            if child.try_wait()?.is_none() {
                child.kill()?;
            }
            let status = child.wait()?;
            self.last_exit = status.code();
        }
        self.overlay_open = false;
        Ok(())
    }

    pub fn overlay(&mut self, open: bool) -> io::Result<()> {
        if open && self.snapshot()?.running_pid.is_none() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "cannot open overlay without a running game"));
        }
        self.overlay_open = open;
        Ok(())
    }

    pub fn execute(&mut self, raw: &str) -> String {
        let result: io::Result<String> = match raw.trim() {
            "PING" => Ok("PONG\n".to_string()),
            "STATUS" => self.snapshot().map(|s| s.to_wire()),
            "GAME START" => self.start().map(|pid| format!("OK GAME START pid={pid}\n")),
            "GAME STOP" => self.stop().map(|_| "OK GAME STOP\n".to_string()),
            "OVERLAY OPEN" => self.overlay(true).map(|_| "OK OVERLAY OPEN\n".to_string()),
            "OVERLAY CLOSE" => self.overlay(false).map(|_| "OK OVERLAY CLOSED\n".to_string()),
            _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "unknown command")),
        };
        match result {
            Ok(reply) => reply,
            Err(err) => format!("ERROR {err}\n"),
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// UI policy is capability-based. Device names only appear in the baseline docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceCapabilities {
    pub memory_budget_mib: u32,
    pub displays: u8,
    pub has_touch: bool,
    pub has_vulkan: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cannot_launch_without_a_configured_game() {
        let mut service = Supervisor::new(None);
        assert!(service.execute("GAME START").starts_with("ERROR "));
        assert!(service.execute("OVERLAY OPEN").starts_with("ERROR "));
        assert_eq!(service.execute("PING"), "PONG\n");
        assert!(service.execute("STATUS").contains("mode=home"));
    }

    #[test]
    fn rejects_unrecognized_commands() {
        let mut service = Supervisor::new(None);
        assert!(service.execute("START /bin/sh").starts_with("ERROR "));
        assert!(service.execute("GAME START; rm -rf /").starts_with("ERROR "));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn child_lifecycle_and_overlay_are_independent() {
        let config = GameConfig {
            program: PathBuf::from("/bin/sleep"),
            args: vec!["60".into()],
        };
        let mut service = Supervisor::new(Some(config));
        assert!(service.execute("GAME START").starts_with("OK "));
        assert!(service.execute("STATUS").contains("mode=game"));
        assert_eq!(service.execute("OVERLAY OPEN"), "OK OVERLAY OPEN\n");
        assert!(service.execute("STATUS").contains("overlay=1"));
        assert_eq!(service.execute("OVERLAY CLOSE"), "OK OVERLAY CLOSED\n");
        assert_eq!(service.execute("GAME STOP"), "OK GAME STOP\n");
        assert!(service.execute("STATUS").contains("mode=home"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn exited_child_is_reaped_and_overlay_resets() {
        let config = GameConfig {
            program: PathBuf::from("/bin/true"),
            args: vec![],
        };
        let mut service = Supervisor::new(Some(config));
        service.start().expect("spawn /bin/true");
        for _ in 0..100 {
            if service.snapshot().expect("poll status").running_pid.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(service.snapshot().unwrap().running_pid.is_none());
        assert_eq!(service.snapshot().unwrap().last_exit, Some(0));
    }
}
