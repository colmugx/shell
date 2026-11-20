// SPDX-License-Identifier: GPL-3.0-or-later
//! IPC client. WATCH keeps the connection open and streams session events.
use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

fn default_socket() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(|runtime| PathBuf::from(runtime).join("pokshell.sock"))
        .unwrap_or_else(|| PathBuf::from("./pokshell.sock"))
}

fn run() -> io::Result<()> {
    let mut socket = default_socket();
    let mut words = Vec::new();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--socket" {
            socket = PathBuf::from(args.next().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "--socket requires a path")
            })?);
        } else if arg == "--help" {
            println!("pokctl [--socket PATH] PING | STATUS | WATCH | GAME START/STOP | OVERLAY OPEN/CLOSE | QUIT");
            return Ok(());
        } else {
            words.push(arg);
        }
    }
    if words.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "missing command (see --help)"));
    }
    let mut stream = UnixStream::connect(&socket)?;
    let request = format!("{}\n", words.join(" "));
    stream.write_all(request.as_bytes())?;
    stream.set_read_timeout(if words.len() == 1 && words[0] == "WATCH" {
        None
    } else {
        Some(std::time::Duration::from_secs(4))
    })?;

    let watch = words.len() == 1 && words[0] == "WATCH";
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        print!("{line}");
        io::stdout().flush()?;
        if line.starts_with("ERROR ") {
            return Err(io::Error::other(line.trim_end().to_owned()));
        }
        if !watch {
            break;
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("pokctl: {error}");
        std::process::exit(1);
    }
}
