// SPDX-License-Identifier: GPL-3.0-or-later
//! Game-first native Linux session control plane.
//!
//! This is process supervision and an event model, NOT a graphics compositor.
use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::io;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;
const STOP_GRACE: Duration = Duration::from_millis(750);

/// The sole raw Linux ABI boundary in the core. Games are launched in their
/// own process group; never signal the caller's or compositor's process group.
mod unix_signal {
    use std::io;

    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }

    pub fn group(pgid: u32, signal: i32) -> io::Result<()> {
        let pgid: i32 = pgid.try_into().map_err(|_| io::Error::other("pid out of range"))?;
        // SAFETY: pid is a positive PID returned by spawn after process_group(0).
        // A negative pid targets that *child's* process group, not this process.
        if unsafe { kill(-pgid, signal) } == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub struct GameConfig {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// Explicit *nested* Wayland socket basename. With Some(..), fail closed
    /// instead of accidentally opening the game under KDE/XWayland.
    pub wayland_display: Option<OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub running_pid: Option<u32>,
    pub overlay_open: bool,
    pub last_exit: Option<i32>,
    pub last_signal: Option<i32>,
}

impl Snapshot {
    /// Preserve old fields so previous pokctl scripts keep working.
    pub fn to_wire(&self) -> String {
        let mode = if self.running_pid.is_some() { "game" } else { "home" };
        let pid = self.running_pid.map(|p| p.to_string()).unwrap_or_else(|| "none".into());
        let exit = self.last_exit.map(|e| e.to_string()).unwrap_or_else(|| "none".into());
        let signal = self.last_signal.map(|s| s.to_string()).unwrap_or_else(|| "none".into());
        format!("mode={mode} overlay={} pid={pid} last_exit={exit} last_signal={signal}\n",
            u8::from(self.overlay_open))
    }
}

/// Typed events support a native UI, a component host, and eventual compositor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    GameStarted { pid: u32 },
    GameExited { pid: u32, code: Option<i32>, signal: Option<i32> },
    OverlayChanged { open: bool },
}

impl SessionEvent {
    pub fn to_wire(&self, seq: u64) -> String {
        match self {
            Self::GameStarted { pid } => format!("EVENT seq={seq} type=game_started pid={pid}\n"),
            Self::GameExited { pid, code, signal } => format!(
                "EVENT seq={seq} type=game_exited pid={pid} code={} signal={}\n",
                code.map(|x| x.to_string()).unwrap_or_else(|| "none".into()),
                signal.map(|x| x.to_string()).unwrap_or_else(|| "none".into())),
            Self::OverlayChanged { open } => format!(
                "EVENT seq={seq} type=overlay_changed open={}\n", u8::from(*open)),
        }
    }
}

/// Owns only a game it created, never an arbitrary system PID. The state
/// machine stays independent of Wayland, fbdev, MoonBit and WAMR.
pub struct Supervisor {
    game_config: Option<GameConfig>,
    game: Option<Child>,
    overlay_open: bool,
    last_exit: Option<i32>,
    last_signal: Option<i32>,
    pending: Vec<SessionEvent>,
}

impl Supervisor {
    pub fn new(game_config: Option<GameConfig>) -> Self {
        Self { game_config, game: None, overlay_open: false,
            last_exit: None, last_signal: None, pending: Vec::new() }
    }

    fn set_overlay(&mut self, open: bool) {
        if self.overlay_open != open {
            self.overlay_open = open;
            self.pending.push(SessionEvent::OverlayChanged { open });
        }
    }

    fn finished(&mut self, status: ExitStatus) {
        if let Some(child) = self.game.take() {
            self.set_overlay(false);
            self.last_exit = status.code();
            self.last_signal = status.signal();
            self.pending.push(SessionEvent::GameExited {
                pid: child.id(),
                code: self.last_exit,
                signal: self.last_signal,
            });
        }
    }

    fn poll(&mut self) -> io::Result<()> {
        let status = match self.game.as_mut() {
            Some(child) => child.try_wait()?,
            None => None,
        };
        if let Some(status) = status {
            self.finished(status);
        }
        Ok(())
    }

    /// Poll every frame/event-loop tick, not just when someone requests STATUS.
    pub fn snapshot(&mut self) -> io::Result<Snapshot> {
        self.poll()?;
        Ok(Snapshot {
            running_pid: self.game.as_ref().map(Child::id),
            overlay_open: self.overlay_open,
            last_exit: self.last_exit,
            last_signal: self.last_signal,
        })
    }

    /// Isolate a game's display from the KDE host. An explicit nested target
    /// is required: without it we preserve the original general-purpose
    /// process-supervisor behavior for non-graphical tests.
    fn configure_game_display(config: &GameConfig, command: &mut Command) -> io::Result<()> {
        let Some(display) = &config.wayland_display else { return Ok(()); };
        let name = display.to_str().ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput, "Wayland socket name must be UTF-8"
        ))?;
        // Never silently accept the host KDE socket (typically wayland-0).
        if !name.starts_with("pokshell-") || name.len() <= "pokshell-".len()
            || name.len() > 70 || !name.bytes().all(|c|
                c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "game Wayland display must be a pokshell-* socket basename"));
        }
        let runtime = env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound,
                "XDG_RUNTIME_DIR is missing for nested Wayland game"))?;
        let runtime = PathBuf::from(runtime);
        if !runtime.is_absolute() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "XDG_RUNTIME_DIR must be an absolute directory"));
        }
        let socket_path = runtime.join(display);
        let metadata = fs::metadata(&socket_path).map_err(|err| io::Error::new(
            err.kind(), format!("nested game socket {} not ready: {err}", socket_path.display())
        ))?;
        if !metadata.file_type().is_socket() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                format!("nested game endpoint is not a socket: {}", socket_path.display())));
        }
        // A successful connect proves there is a listening server, not just a
        // stale socket file; it is intentionally not a Wayland protocol probe.
        let _stream = UnixStream::connect(&socket_path).map_err(|err| io::Error::new(
            err.kind(), format!("cannot connect to nested game socket {}: {err}", socket_path.display())
        ))?;
        command.env("WAYLAND_DISPLAY", display)
            .env("SDL_VIDEODRIVER", "wayland")
            .env("GDK_BACKEND", "wayland")
            .env("QT_QPA_PLATFORM", "wayland")
            .env("XDG_SESSION_TYPE", "wayland")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_SOCKET"); // inherited fd overrides WAYLAND_DISPLAY
        eprintln!("pokshell: game transport nested_wayland={name} x11_fallback=disabled");
        Ok(())
    }

    pub fn start(&mut self) -> io::Result<u32> {
        if let Some(pid) = self.snapshot()?.running_pid {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists,
                format!("game already running: {pid}")));
        }
        let config = self.game_config.as_ref().ok_or_else(|| io::Error::new(
            io::ErrorKind::NotFound, "no game configured (--game PATH)"))?;
        let mut command = Command::new(&config.program);
        command.args(&config.args)
            .stdin(Stdio::null())
            // A native game can stall before creating its first XDG surface.
            // Keep stdout as well as stderr in the session log; otherwise
            // SDL/EGL startup diagnostics are silently discarded.
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        Self::configure_game_display(config, &mut command)?;
        command.process_group(0); // Safe std API, available since Rust 1.64.
        let child = command.spawn()?;
        let pid = child.id();
        self.game = Some(child);
        self.last_exit = None;
        self.last_signal = None;
        self.set_overlay(false);
        self.pending.push(SessionEvent::GameStarted { pid });
        Ok(pid)
    }

    /// SIGTERM whole child process group, wait, then SIGKILL after deadline.
    /// A process that exits voluntarily still gets a GameExited event.
    pub fn stop(&mut self) -> io::Result<()> {
        self.poll()?;
        if let Some(pid) = self.game.as_ref().map(Child::id) {
            if let Err(err) = unix_signal::group(pid, SIGTERM) {
                // ESRCH means the group exited between try_wait and signal.
                if err.raw_os_error() != Some(3) {
                    return Err(err);
                }
            }
            let until = Instant::now() + STOP_GRACE;
            loop {
                self.poll()?;
                if self.game.is_none() {
                    break;
                }
                if Instant::now() >= until {
                    // Even if the leader ignores SIGTERM, all its descendants
                    // receive SIGKILL through the isolated process group.
                    if let Err(err) = unix_signal::group(pid, SIGKILL) {
                        if err.raw_os_error() != Some(3) {
                            return Err(err);
                        }
                    }
                    if let Some(mut child) = self.game.take() {
                        let status = child.wait()?;
                        self.game = Some(child);
                        self.finished(status);
                    }
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
        self.set_overlay(false);
        Ok(())
    }

    pub fn overlay(&mut self, open: bool) -> io::Result<()> {
        if open && self.snapshot()?.running_pid.is_none() {
            return Err(io::Error::new(io::ErrorKind::NotFound,
                "cannot open overlay without a running game"));
        }
        self.set_overlay(open);
        Ok(())
    }

    /// The event queue is drained by the server after each command and tick.
    /// It is not a persistent replay log.
    pub fn take_events(&mut self) -> Vec<SessionEvent> {
        std::mem::take(&mut self.pending)
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

/// UI policy is capability-based, not bound to H700/QCS6125/SM8550 names.
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
    use std::path::PathBuf;

    #[test]
    fn no_arbitrary_execution_or_overlay_without_game() {
        let mut service = Supervisor::new(None);
        assert!(service.execute("GAME START").starts_with("ERROR "));
        assert!(service.execute("OVERLAY OPEN").starts_with("ERROR "));
        assert!(service.execute("GAME START; /bin/sh").starts_with("ERROR "));
        assert_eq!(service.execute("PING"), "PONG\n");
        assert!(service.execute("STATUS").contains("mode=home"));
        assert!(service.take_events().is_empty());
    }

    #[test]
    fn event_wire_protocol_is_stable() {
        assert_eq!(SessionEvent::GameStarted {pid: 12}.to_wire(1),
            "EVENT seq=1 type=game_started pid=12\n");
        assert_eq!(SessionEvent::OverlayChanged {open: true}.to_wire(2),
            "EVENT seq=2 type=overlay_changed open=1\n");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn lifecycle_events_and_signal_exit() {
        let mut service = Supervisor::new(Some(GameConfig {
            program: PathBuf::from("/bin/sleep"), args: vec!["120".into()],
            wayland_display: None,
        }));
        let pid = service.start().unwrap();
        assert_eq!(service.take_events(), vec![SessionEvent::GameStarted {pid}]);
        service.overlay(true).unwrap();
        assert_eq!(service.take_events(), vec![SessionEvent::OverlayChanged {open:true}]);
        service.stop().unwrap();
        let events = service.take_events();
        assert!(events.contains(&SessionEvent::OverlayChanged {open:false}));
        assert!(events.iter().any(|e| matches!(e, SessionEvent::GameExited {pid: p, ..} if *p == pid)));
        let status = service.snapshot().unwrap();
        assert!(status.running_pid.is_none());
        assert_eq!(status.last_signal, Some(SIGTERM));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn explicit_wayland_display_is_fail_closed() {
        let mut service = Supervisor::new(Some(GameConfig {
            program: PathBuf::from("/bin/sleep"),
            args: vec!["120".into()],
            wayland_display: Some("wayland-0".into()),
        }));
        assert!(service.execute("GAME START").contains("pokshell-*"));
        assert!(service.snapshot().unwrap().running_pid.is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn spontaneous_exit_is_reaped_and_reported() {
        let mut service = Supervisor::new(Some(GameConfig {
            program: PathBuf::from("/bin/false"), args: vec![],
            wayland_display: None,
        }));
        let pid = service.start().unwrap();
        service.take_events();
        for _ in 0..100 {
            if service.snapshot().unwrap().running_pid.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(service.snapshot().unwrap().last_exit, Some(1));
        assert_eq!(service.take_events(), vec![SessionEvent::GameExited {
            pid, code:Some(1), signal:None,
        }]);
    }
}
