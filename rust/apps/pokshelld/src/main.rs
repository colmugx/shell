// SPDX-License-Identifier: GPL-3.0-or-later
//! Game-first user session process, without a desktop or framebuffer dependency.
use pokshell_core::{GameConfig, Supervisor};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

fn default_socket() -> PathBuf {
    if let Some(runtime) = env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime).join("pokshell.sock")
    } else {
        PathBuf::from("./pokshell.sock") // Development fallback; not for a system service.
    }
}

fn args() -> Result<(PathBuf, Option<GameConfig>), String> {
    let mut socket = default_socket();
    let mut program: Option<PathBuf> = None;
    let mut game_args: Vec<OsString> = Vec::new();
    let mut argv = env::args_os().skip(1);
    while let Some(arg) = argv.next() {
        match arg.to_str() {
            Some("--socket") => socket = PathBuf::from(argv.next().ok_or("--socket needs a path")?),
            Some("--game") => program = Some(PathBuf::from(argv.next().ok_or("--game needs a path")?)),
            Some("--game-arg") => game_args.push(argv.next().ok_or("--game-arg needs a value")?),
            Some("--help") => {
                println!("pokshelld [--socket PATH] [--game EXECUTABLE] [--game-arg ARG ...]");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {}", arg.to_string_lossy())),
        }
    }
    Ok((socket, program.map(|program| GameConfig { program, args: game_args })))
}

fn serve_connection(mut stream: UnixStream, supervisor: &mut Supervisor) -> io::Result<bool> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut data = [0_u8; 256];
    let length = stream.read(&mut data)?;
    if length == 0 {
        return Ok(false);
    }
    let request = match std::str::from_utf8(&data[..length]) {
        Ok(s) if s.ends_with('\n') && length < data.len() => s.trim(),
        _ => {
            stream.write_all(b"ERROR invalid or oversized request\n")?;
            return Ok(false);
        }
    };
    let (reply, quit) = if request == "QUIT" {
        ("OK QUIT\n".to_string(), true)
    } else {
        (supervisor.execute(request), false)
    };
    stream.write_all(reply.as_bytes())?;
    Ok(quit)
}

fn run(socket: &Path, game: Option<GameConfig>) -> io::Result<()> {
    if socket.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists,
            format!("socket path already exists: {} (check stale socket manually)", socket.display())));
    }
    let listener = UnixListener::bind(socket)?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let mut supervisor = Supervisor::new(game);
    eprintln!("pokshelld ready socket={}", socket.display());

    let result = (|| -> io::Result<()> {
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    if serve_connection(stream, &mut supervisor)? {
                        break;
                    }
                }
                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => (),
                Err(err) => return Err(err),
            }
            let _ = supervisor.snapshot()?; // child cleanup even without clients
            thread::sleep(Duration::from_millis(25));
        }
        Ok(())
    })();
    // Explicit drop closes the UnixListener before unlinking the socket.
    drop(listener);
    let _ = fs::remove_file(socket);
    result
}

fn main() {
    let (socket, game) = args().unwrap_or_else(|reason| {
        eprintln!("{reason}");
        std::process::exit(2);
    });
    if let Err(err) = run(&socket, game) {
        eprintln!("pokshelld: {err}");
        std::process::exit(1);
    }
}
