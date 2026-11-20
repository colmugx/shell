// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux user-session daemon: bounded line protocol and live event subscribers.
//! The control plane is graphical-backend agnostic.
use pokshell_core::{GameConfig, Supervisor};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// The signal handler does only a lock-free atomic store. The main event loop
// performs all socket cleanup and game-process termination outside the handler.
static SIGNAL_SHUTDOWN: AtomicBool = AtomicBool::new(false);

const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

extern "C" fn request_shutdown(_signal: i32) {
    SIGNAL_SHUTDOWN.store(true, Ordering::Release);
}

fn install_shutdown_handlers() -> io::Result<()> {
    unsafe extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }
    // SAFETY: libc signal() is called before the daemon starts worker threads.
    // Its callbacks only perform an atomic store; signal errors are checked.
    for number in [SIGINT, SIGTERM] {
        if unsafe { signal(number, request_shutdown as usize) } == usize::MAX {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

const MAX_LINE: usize = 256;
const MAX_CLIENTS: usize = 32;
const MAX_WATCHERS: usize = 16;
const WATCH_QUEUE: usize = 64;

struct Service {
    supervisor: Supervisor,
    subscribers: Vec<(u64, SyncSender<String>)>,
    next_subscriber: u64,
    next_event_seq: u64,
}

impl Service {
    fn new(game: Option<GameConfig>) -> Self {
        Self { supervisor: Supervisor::new(game), subscribers: Vec::new(),
            next_subscriber: 1, next_event_seq: 1 }
    }

    fn broadcast_pending(&mut self) {
        for event in self.supervisor.take_events() {
            let line = event.to_wire(self.next_event_seq);
            self.next_event_seq += 1;
            // Drop subscribers which disconnected or failed to read promptly.
            // This avoids an unbounded queue delaying the game/session thread.
            self.subscribers.retain(|(_, sender)| {
                match sender.try_send(line.clone()) {
                    Ok(()) => true,
                    Err(TrySendError::Disconnected(_)) | Err(TrySendError::Full(_)) => false,
                }
            });
        }
    }

    fn tick(&mut self) -> io::Result<()> {
        self.supervisor.snapshot()?;
        self.broadcast_pending();
        Ok(())
    }

    fn command(&mut self, request: &str) -> String {
        let reply = self.supervisor.execute(request);
        self.broadcast_pending();
        reply
    }

    fn subscribe(&mut self) -> io::Result<(u64, Receiver<String>, String)> {
        if self.subscribers.len() >= MAX_WATCHERS {
            return Err(io::Error::other("too many subscribers"));
        }
        self.tick()?;
        let snapshot = self.supervisor.snapshot()?.to_wire();
        let id = self.next_subscriber;
        self.next_subscriber += 1;
        let (tx, rx) = mpsc::sync_channel(WATCH_QUEUE);
        self.subscribers.push((id, tx));
        Ok((id, rx, snapshot))
    }

    fn unsubscribe(&mut self, id: u64) {
        self.subscribers.retain(|(subscriber, _)| *subscriber != id);
    }
}

fn default_socket() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(|x| PathBuf::from(x).join("pokshell.sock"))
        .unwrap_or_else(|| PathBuf::from("./pokshell.sock"))
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

/// Read an entire newline-terminated command, including fragmented writes,
/// while bounding total bytes and limiting the per-client read duration.
fn read_command(stream: &UnixStream) -> io::Result<Option<String>> {
    // Reading one byte at a time keeps the protocol bounded even if a hostile
    // client never sends a newline. SO_RCVTIMEO (2s) bounds a stalled client.
    let mut source = stream;
    let mut packet = Vec::with_capacity(64);
    loop {
        let mut byte = [0u8; 1];
        match source.read(&mut byte)? {
            0 if packet.is_empty() => return Ok(None),
            0 => return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                    "command missing newline")),
            _ => {
                packet.push(byte[0]);
                if packet.len() > MAX_LINE {
                    return Err(io::Error::new(io::ErrorKind::InvalidData,
                        "command exceeds 256 bytes"));
                }
                if byte[0] == b'\n' { break; }
            }
        }
    }
    let raw = std::str::from_utf8(&packet).map_err(|_| io::Error::new(
        io::ErrorKind::InvalidData, "command is not UTF-8"))?;
    Ok(Some(raw.trim().to_string()))
}

fn watch(mut stream: UnixStream, service: &Arc<Mutex<Service>>,
    shutdown: &AtomicBool) -> io::Result<()> {
    let (id, receiver, initial) = service.lock()
        .map_err(|_| io::Error::other("session lock poisoned"))?.subscribe()?;
    let result = (|| -> io::Result<()> {
        stream.write_all(format!("OK WATCH\n{initial}").as_bytes())?;
        while !shutdown.load(Ordering::Acquire) {
            match receiver.recv_timeout(Duration::from_secs(1)) {
                Ok(message) => stream.write_all(message.as_bytes())?,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // A heartbeat detects a disconnected watcher even if the
                    // session has no events for a long time.
                    stream.write_all(b"KEEPALIVE\n")?;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    stream.write_all(b"ERROR watch queue overflow; reconnect\n")?;
                    break;
                }
            }
        }
        Ok(())
    })();
    if let Ok(mut session) = service.lock() {
        session.unsubscribe(id);
    }
    result
}

fn serve_connection(mut stream: UnixStream, service: Arc<Mutex<Service>>,
    shutdown: Arc<AtomicBool>) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = match read_command(&stream) {
        Ok(Some(request)) => request,
        Ok(None) => return Ok(()),
        Err(err) => {
            let _ = stream.write_all(format!("ERROR {err}\n").as_bytes());
            return Ok(());
        }
    };
    if request == "WATCH" {
        if let Err(err) = watch(stream.try_clone()?, &service, &shutdown) {
            if err.kind() != io::ErrorKind::BrokenPipe {
                let _ = stream.write_all(format!("ERROR {err}\n").as_bytes());
            }
        }
        return Ok(());
    }
    let response = if request == "QUIT" {
        shutdown.store(true, Ordering::Release);
        "OK QUIT\n".to_string()
    } else {
        service.lock().map_err(|_| io::Error::other("session lock poisoned"))?
            .command(&request)
    };
    stream.write_all(response.as_bytes())
}

fn run(socket: &Path, game: Option<GameConfig>) -> io::Result<()> {
    // Do not unlink a pre-existing path: it might belong to another process.
    if socket.symlink_metadata().is_ok() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists,
            format!("socket already exists: {} (remove stale socket yourself)", socket.display())));
    }
    let listener = UnixListener::bind(socket)?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;

    let service = Arc::new(Mutex::new(Service::new(game)));
    let shutdown = Arc::new(AtomicBool::new(false));
    let active = Arc::new(AtomicUsize::new(0));
    eprintln!("pokshelld ready socket={}", socket.display());

    let result = (|| -> io::Result<()> {
        while !shutdown.load(Ordering::Acquire) && !SIGNAL_SHUTDOWN.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    if active.fetch_add(1, Ordering::AcqRel) >= MAX_CLIENTS {
                        active.fetch_sub(1, Ordering::AcqRel);
                        let _ = stream.write_all(b"ERROR too many connections\n");
                        continue;
                    }
                    let service_ref = Arc::clone(&service);
                    let stop_ref = Arc::clone(&shutdown);
                    let active_ref = Arc::clone(&active);
                    thread::spawn(move || {
                        if let Err(err) = serve_connection(stream, service_ref, stop_ref) {
                            eprintln!("pokshelld: client: {err}");
                        }
                        active_ref.fetch_sub(1, Ordering::AcqRel);
                    });
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => (),
                Err(err) => return Err(err),
            }
            service.lock().map_err(|_| io::Error::other("session lock poisoned"))?
                .tick()?;
            thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    })();

    // Stop WATCH subscribers on every exit path, including external signals.
    shutdown.store(true, Ordering::Release);
    // QUIT, SIGINT and SIGTERM stop the game before this daemon exits.
    // QUIT stops the game before this daemon process exits. Do not rely on
    // worker-thread destructors: process exit does not wait for threads.
    if let Ok(mut state) = service.lock() {
        let _ = state.supervisor.stop();
        state.broadcast_pending();
    }
    drop(listener);
    let _ = fs::remove_file(socket);
    result
}

fn main() {
    let (socket, game) = args().unwrap_or_else(|reason| {
        eprintln!("{reason}");
        std::process::exit(2);
    });
    if let Err(err) = install_shutdown_handlers().and_then(|()| run(&socket, game)) {
        eprintln!("pokshelld: {err}");
        std::process::exit(1);
    }
}
