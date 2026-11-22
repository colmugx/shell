// SPDX-License-Identifier: GPL-3.0-or-later
//! PocketJS v0.17 native service ops 30/31/32, injected into the pinned
//! Linux host at build time. No changes to the upstream runtime API.
use pocket_runtime::{OpArg, OpProvider, OpResult, RuntimeError};
use pocketjs_core::Ui;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

const REPLY_LIMIT: usize = 512;
const QUEUE_LIMIT: usize = 16;
const INPUT_LIMIT: usize = 256;
const IPC_TIMEOUT: Duration = Duration::from_millis(200);

pub struct PokShellService {
    socket: PathBuf,
    opened: bool,
    pending: VecDeque<String>,
}

impl PokShellService {
    pub fn from_environment() -> Option<Self> {
        let socket = PathBuf::from(std::env::var_os("POKSHELL_SESSION_SOCKET")?);
        if !socket.is_absolute() {
            eprintln!("pokshell-svc: refusing relative session socket");
            return None;
        }
        Some(Self { socket, opened: false, pending: VecDeque::new() })
    }

    fn socket_is_safe(&self) -> bool {
        fs::symlink_metadata(&self.socket)
            .is_ok_and(|meta| meta.file_type().is_socket())
    }

    fn request(&self, command: &str) -> Result<String, &'static str> {
        if !self.socket_is_safe() { return Err("session unavailable"); }
        let mut socket = UnixStream::connect(&self.socket).map_err(|_| "session offline")?;
        socket.set_read_timeout(Some(IPC_TIMEOUT)).map_err(|_| "session offline")?;
        socket.set_write_timeout(Some(IPC_TIMEOUT)).map_err(|_| "session offline")?;
        socket.write_all(command.as_bytes()).map_err(|_| "session write failed")?;
        socket.write_all(b"\n").map_err(|_| "session write failed")?;
        let mut response = Vec::with_capacity(80);
        let mut byte = [0u8; 1];
        loop {
            if response.len() >= REPLY_LIMIT { return Err("oversized session reply"); }
            match socket.read(&mut byte) {
                Ok(1) if byte[0] == b'\n' => break,
                Ok(1) => response.push(byte[0]),
                Ok(_) => return Err("incomplete session reply"),
                Err(_) => return Err("session read timeout"),
            }
        }
        String::from_utf8(response).map_err(|_| "invalid session reply")
    }

    fn parse_action(line: &str) -> Result<(i64, &'static str), &'static str> {
        if line.len() > INPUT_LIMIT { return Err("oversized request"); }
        let v: Value = serde_json::from_str(line).map_err(|_| "invalid JSON")?;
        let obj = v.as_object().ok_or("invalid request")?;
        if obj.len() != 2 || !obj.contains_key("t") || !obj.contains_key("id") {
            return Err("only t and id are permitted");
        }
        let id = obj["id"].as_i64().ok_or("invalid request id")?;
        if !(-2_000_000..=2_000_000).contains(&id) { return Err("invalid request id"); }
        let action = match obj["t"].as_str() {
            Some("status") => "STATUS",
            Some("game.start") => "GAME START",
            Some("game.stop") => "GAME STOP",
            Some("overlay.open") => "OVERLAY OPEN",
            Some("overlay.close") => "OVERLAY CLOSE",
            _ => return Err("unsupported action"),
        };
        Ok((id, action))
    }

    fn send(&mut self, line: &str) {
        if self.pending.len() >= QUEUE_LIMIT {
            self.pending.pop_front();
        }
        let reply = match Self::parse_action(line) {
            Ok((id, command)) => {
                // This is the complete privileged command allowlist.
                // Game executable selection stays in Rust session config.
                let result = self.request(command);
                let success = result.as_ref().is_ok_and(|v| !v.starts_with("ERROR"));
                if success {
                    eprintln!("pokshell-svc: action={command} accepted");
                    json!({"id": id, "t": "result", "action": command,
                           "value": result.unwrap()})
                } else {
                    json!({"id": id, "t": "error",
                           "message": result.unwrap_or_else(|err| err.to_string())})
                }
            }
            Err(reason) => json!({"t": "error", "message": reason}),
        };
        self.pending.push_back(reply.to_string());
    }
}

impl OpProvider for PokShellService {
    fn supports_op(&self, op: u16) -> bool { (30..=32).contains(&op) }

    fn dispatch(&mut self, _: &mut Ui, op: u16, args: &[OpArg<'_>])
        -> Result<OpResult, RuntimeError>
    {
        match (op, args) {
            (30, [OpArg::Bytes(name)]) => {
                let valid = name == b"pokshell" && self.socket_is_safe();
                self.opened = valid;
                Ok(OpResult::Integer(i64::from(valid)))
            },
            (31, []) => {
                if !self.opened { return Ok(OpResult::None); }
                Ok(match self.pending.pop_front() {
                    Some(reply) => OpResult::Bytes(reply.into_bytes()),
                    None => OpResult::None,
                })
            },
            (32, [OpArg::Bytes(line)]) => {
                if !self.opened { return Err(RuntimeError::InvalidState); }
                let raw = std::str::from_utf8(line).map_err(|_| RuntimeError::InvalidArgument)?;
                self.send(raw);
                Ok(OpResult::None)
            },
            _ => Err(RuntimeError::InvalidArgument),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unlisted_commands_and_extra_fields() {
        for raw in [
            r#"{"t":"quit","id":1}"#,
            r#"{"t":"shell","id":1,"cmd":"rm"}"#,
            r#"{"t":"game.start","id":1,"path":"/bin/sh"}"#,
            r#"{"t":"power.off","id":1}"#,
            r#"{"t":"status","id":"1"}"#,
            r#"{"t":"status","id":999999999}"#,
            "{",
        ] {
            assert!(PokShellService::parse_action(raw).is_err(), "{raw}");
        }
        assert_eq!(PokShellService::parse_action(r#"{"t":"game.start","id":-1}"#).unwrap(),
                   (-1, "GAME START"));
        assert_eq!(PokShellService::parse_action(r#"{"t":"status","id":-2}"#).unwrap(),
                   (-2, "STATUS"));
    }
}
