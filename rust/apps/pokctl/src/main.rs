// SPDX-License-Identifier: GPL-3.0-or-later
use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

fn default_socket() -> PathBuf {
    if let Some(runtime) = env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime).join("pokshell.sock")
    } else {
        PathBuf::from("./pokshell.sock")
    }
}

fn main() {
    let mut socket = default_socket();
    let mut words: Vec<String> = Vec::new();
    let mut argv = env::args().skip(1);
    while let Some(arg) = argv.next() {
        if arg == "--socket" {
            socket = PathBuf::from(argv.next().unwrap_or_else(|| {
                eprintln!("--socket requires a path");
                std::process::exit(2);
            }));
        } else if arg == "--help" {
            println!("pokctl [--socket PATH] PING | STATUS | GAME START/STOP | OVERLAY OPEN/CLOSE | QUIT");
            return;
        } else {
            words.push(arg);
        }
    }
    if words.is_empty() {
        eprintln!("missing command; see --help");
        std::process::exit(2);
    }
    let mut stream = UnixStream::connect(&socket).unwrap_or_else(|error| {
        eprintln!("{}: {error}", socket.display());
        std::process::exit(1);
    });
    let request = format!("{}\n", words.join(" "));
    if let Err(error) = stream.write_all(request.as_bytes()) {
        eprintln!("IPC write error: {error}");
        std::process::exit(1);
    }
    let mut reply = String::new();
    if let Err(error) = stream.read_to_string(&mut reply) {
        eprintln!("IPC read error: {error}");
        std::process::exit(1);
    }
    print!("{reply}");
    if reply.starts_with("ERROR ") {
        std::process::exit(1);
    }
}
