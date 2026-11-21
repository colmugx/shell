// SPDX-License-Identifier: GPL-3.0-or-later
//! One-command nested graphical session supervisor for regular KDE Wayland users.
//! Keeps the host compositor independent, pins game + UI to pokcomp, and owns
//! only child processes started by this session (never a foreign KDE process).
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

extern "C" fn handle_signal(_: i32) {
    SHUTDOWN.store(true, Ordering::Release);
}

fn install_signals() -> io::Result<()> {
    unsafe extern "C" { fn signal(number: i32, handler: usize) -> usize; }
    for number in [SIGINT, SIGTERM] {
        // SAFETY: a single atomic store is async-signal safe.
        if unsafe { signal(number, handle_signal as usize) } == usize::MAX {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    game: Option<PathBuf>,
    game_args: Vec<OsString>,
    compositor: Option<PathBuf>,
    daemon: Option<PathBuf>,
    layer: Option<PathBuf>,
    ipc_socket: Option<PathBuf>,
    no_autostart: bool,
}

fn parse_options(args: impl IntoIterator<Item = OsString>) -> Result<Option<Options>, String> {
    let mut args = args.into_iter();
    let mut opts = Options::default();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--game") if opts.game.is_none() =>
                opts.game = Some(PathBuf::from(args.next().ok_or("--game requires a path")?)),
            Some("--game-arg") => opts.game_args.push(args.next().ok_or("--game-arg requires a value")?),
            Some("--compositor") if opts.compositor.is_none() =>
                opts.compositor = Some(PathBuf::from(args.next().ok_or("--compositor requires a path")?)),
            Some("--daemon") if opts.daemon.is_none() =>
                opts.daemon = Some(PathBuf::from(args.next().ok_or("--daemon requires a path")?)),
            Some("--layer") if opts.layer.is_none() =>
                opts.layer = Some(PathBuf::from(args.next().ok_or("--layer requires a path")?)),
            Some("--socket") if opts.ipc_socket.is_none() =>
                opts.ipc_socket = Some(PathBuf::from(args.next().ok_or("--socket requires a path")?)),
            Some("--no-autostart") if !opts.no_autostart => opts.no_autostart = true,
            Some("--help") | Some("-h") => return Ok(None),
            _ => return Err(format!("unknown/duplicate argument: {}", arg.to_string_lossy())),
        }
    }
    if opts.game.is_none() {
        return Err("--game PATH is required (e.g. /usr/games/supertux)".into());
    }
    Ok(Some(opts))
}

fn validate_display(name: &str) -> io::Result<()> {
    if !name.starts_with("pokshell-") || name.len() <= "pokshell-".len()
        || name.len() > 70
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(io::Error::new(io::ErrorKind::InvalidData,
            "pokcomp did not provide a safe nested pokshell-* display name"));
    }
    Ok(())
}

fn send_ipc(path: &Path, request: &str) -> io::Result<String> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(request.as_bytes())?;
    stream.write_all(b"\n")?;
    let mut reply = String::new();
    BufReader::new(stream.take(1024)).read_line(&mut reply)?;
    if reply.is_empty() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "empty pokshelld response"));
    }
    Ok(reply)
}

fn wait_socket(path: &Path, child: &mut Child, label: &str) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        if let Ok(meta) = fs::symlink_metadata(path) {
            if meta.file_type().is_socket() && UnixStream::connect(path).is_ok() {
                return Ok(());
            }
        }
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!("{label} exited early: {status}")));
        }
        thread::sleep(Duration::from_millis(30));
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, format!("{label} socket not ready: {}", path.display())))
}

fn wait_display(child: &mut Child) -> io::Result<String> {
    let pipe = child.stdout.take().ok_or_else(|| io::Error::other("pokcomp stdout pipe missing"))?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            match line {
                Ok(line) => {
                    if let Some(name) = line.strip_prefix("WAYLAND_DISPLAY=") {
                        let _ = tx.send(name.trim().to_owned());
                    } else {
                        eprintln!("pokcomp stdout: {line}");
                    }
                }
                Err(_) => break,
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if SHUTDOWN.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "session interrupted"));
        }
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!("pokcomp exited before announcing display: {status}")));
        }
        if let Ok(display) = rx.recv_timeout(Duration::from_millis(100)) {
            validate_display(&display)?;
            return Ok(display);
        }
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, "pokcomp did not announce its display"))
}

/// Use TERM first so pokcomp's socket cleanup and layer exit paths run.
fn stop_child(child: &mut Child, label: &str) {
    if child.try_wait().ok().flatten().is_some() { return; }
    unsafe extern "C" { fn kill(pid: i32, signal: i32) -> i32; }
    // SAFETY: the PID belongs to a process spawned by this instance.
    let _ = unsafe { kill(child.id() as i32, SIGTERM) };
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            _ if Instant::now() < deadline => thread::sleep(Duration::from_millis(40)),
            _ => break,
        }
    }
    eprintln!("pokshell-session: {label} did not exit on TERM; killing");
    let _ = child.kill();
    let _ = child.wait();
}

fn paths() -> io::Result<(PathBuf, PathBuf, PathBuf)> {
    let exe = env::current_exe()?.canonicalize()?;
    let bin = exe.parent().ok_or_else(|| io::Error::other("binary directory unavailable"))?;
    let rust = bin.parent().and_then(Path::parent)
        .ok_or_else(|| io::Error::other("rust workspace not found relative to binary"))?;
    let root = rust.parent().ok_or_else(|| io::Error::other("repository root unavailable"))?;
    Ok((
        root.join("rust/compositor/target/debug/pokcomp"),
        bin.join("pokshelld"),
        root.join("graphics/wayland-layer/target/debug/pokshell-layer"),
    ))
}

fn session(opts: Options) -> io::Result<()> {
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from).ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR is required"))?;
    if !runtime.is_absolute() || !runtime.is_dir() {
        return Err(io::Error::other("XDG_RUNTIME_DIR must be an existing absolute directory"));
    }
    if env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err(io::Error::other("launch from a regular user's KDE Wayland Konsole"));
    }
    let ipc = opts.ipc_socket.unwrap_or_else(|| runtime.join("pokshell.sock"));
    if !ipc.is_absolute() {
        return Err(io::Error::other("--socket must be an absolute path"));
    }
    if ipc.symlink_metadata().is_ok() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists,
            format!("IPC socket {} already exists; refusing to replace a running daemon", ipc.display())));
    }
    let (default_comp, default_daemon, default_layer) = paths()?;
    let comp_path = opts.compositor.unwrap_or(default_comp);
    let daemon_path = opts.daemon.unwrap_or(default_daemon);
    let layer_path = opts.layer.unwrap_or(default_layer);
    let game = opts.game.expect("validated game path");

    // pokcomp is a *nested* host window, not a DRM/KMS/seat owner.
    let mut compositor = Command::new(&comp_path)
        .env_remove("WAYLAND_SOCKET")
        .stdout(Stdio::piped())
        .spawn()?;
    let mut daemon: Option<Child> = None;
    let mut layer: Option<Child> = None;
    let result = (|| -> io::Result<()> {
        let display = wait_display(&mut compositor)?;
        let graphical_socket = runtime.join(&display);
        wait_socket(&graphical_socket, &mut compositor, "pokcomp")?;

        let mut process = Command::new(&daemon_path);
        process.arg("--socket").arg(&ipc)
            .arg("--game").arg(&game)
            .arg("--game-wayland-display").arg(&display);
        for arg in opts.game_args {
            process.arg("--game-arg").arg(arg);
        }
        // A launched game is pinned to the nested display by pokshelld.
        daemon = Some(process.spawn()?);
        wait_socket(&ipc, daemon.as_mut().expect("spawned"), "pokshelld")?;

        // Explicit display is mandatory. Host KDE WAYLAND_DISPLAY and
        // WAYLAND_SOCKET cannot redirect the menu to another compositor.
        layer = Some(Command::new(&layer_path)
            .arg("--menu").arg("--display").arg(&display)
            .arg("--socket").arg(&ipc)
            .env_remove("WAYLAND_SOCKET")
            .spawn()?);
        // Let the client establish its first layer-shell configure, and
        // reject immediately-crashing clients before starting a game.
        for _ in 0..20 {
            if let Some(status) = layer.as_mut().expect("spawned").try_wait()? {
                return Err(io::Error::other(format!("pokshell-layer exited during startup: {status}")));
            }
            thread::sleep(Duration::from_millis(50));
        }
        if !opts.no_autostart {
            let response = send_ipc(&ipc, "GAME START")?;
            if !response.starts_with("OK GAME START") {
                return Err(io::Error::other(format!("GAME START rejected: {}", response.trim())));
            }
        }
        println!("POKSESSION_READY display={display} ipc={} game={}",
            ipc.display(), game.display());
        println!("Use rust/target/debug/pokctl OVERLAY OPEN to show the menu; Ctrl+C exits the whole nested session.");

        loop {
            if SHUTDOWN.load(Ordering::Acquire) { break; }
            if let Some(status) = compositor.try_wait()? {
                eprintln!("pokshell-session: compositor exited: {status}");
                break;
            }
            if let Some(client) = layer.as_mut() {
                if let Some(status) = client.try_wait()? {
                    return Err(io::Error::other(format!("pokshell-layer exited unexpectedly: {status}")));
                }
            }
            if let Some(process) = daemon.as_mut() {
                if let Some(status) = process.try_wait()? {
                    // pokctl QUIT intentionally leaves the compositor up.
                    eprintln!("pokshell-session: pokshelld exited ({status}); Smithay remains open");
                    daemon = None;
                }
            }
            thread::sleep(Duration::from_millis(80));
        }
        Ok(())
    })();

    if daemon.is_some() {
        // QUIT stops the supervised game and removes the daemon's IPC socket.
        let _ = send_ipc(&ipc, "QUIT");
    }
    if let Some(mut client) = layer { stop_child(&mut client, "layer"); }
    if let Some(mut process) = daemon { stop_child(&mut process, "daemon"); }
    stop_child(&mut compositor, "compositor");
    result
}

fn main() {
    let opts = match parse_options(env::args_os().skip(1)) {
        Ok(Some(opts)) => opts,
        Ok(None) => {
            println!("poksession --game EXECUTABLE [--game-arg ARG] [--no-autostart] [--socket IPC_PATH] [--compositor PATH] [--daemon PATH] [--layer PATH]\nStarts one nested Smithay + pokshelld + system menu session. Ctrl+C terminates owned children.");
            return;
        }
        Err(err) => { eprintln!("poksession: {err}"); std::process::exit(2); }
    };
    if let Err(err) = install_signals().and_then(|()| session(opts)) {
        eprintln!("poksession: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn opts(args: &[&str]) -> Result<Option<Options>, String> {
        parse_options(args.iter().map(OsString::from))
    }
    #[test]
    fn cli_requires_game_and_valid_flags() {
        assert!(opts(&[]).is_err());
        assert!(opts(&["--game"]).is_err());
        assert!(opts(&["--game", "/usr/games/supertux", "--game", "another"]).is_err());
        let p = opts(&["--game", "/usr/games/supertux", "--game-arg", "--fullscreen",
            "--no-autostart"]).unwrap().unwrap();
        assert_eq!(p.game.unwrap(), PathBuf::from("/usr/games/supertux"));
        assert_eq!(p.game_args, [OsString::from("--fullscreen")]);
        assert!(p.no_autostart);
    }
    #[test]
    fn nested_server_name_rejects_host_display_and_paths() {
        for name in ["wayland-0", "pokshell-", "pokshell/0", "../pokshell-1", ""] {
            assert!(validate_display(name).is_err());
        }
        assert!(validate_display("pokshell-1").is_ok());
        assert!(validate_display("pokshell-test").is_ok());
    }
    #[test]
    fn sends_one_ipc_line_and_reads_reply() {
        use std::os::unix::net::UnixListener;
        let dir = std::env::temp_dir().join(format!("pokshell-session-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let sock = dir.join("test.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let worker = thread::spawn(move || {
            let (connection, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(connection);
            let mut command = String::new();
            reader.read_line(&mut command).unwrap();
            assert_eq!(command, "GAME START\n");
            reader.get_mut().write_all(b"OK GAME START pid=42\n").unwrap();
        });
        assert_eq!(send_ipc(&sock, "GAME START").unwrap(), "OK GAME START pid=42\n");
        worker.join().unwrap();
        let _ = fs::remove_file(sock);
        let _ = fs::remove_dir(dir);
    }
}
