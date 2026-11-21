// SPDX-License-Identifier: GPL-3.0-or-later
//! Nested regression backend for the shared Wayland core.
use std::error::Error;
use pokshell_wayland::Server;
use smithay::{backend::{renderer::gles::GlesRenderer, winit::{self, WinitEvent}}, output::Mode, utils::{Rectangle, Transform}};
use ::winit::platform::pump_events::PumpStatus;
const HELP: &str = "pokcomp [--socket NAME]\n\
    Starts a nested Wayland compositor in a host Wayland/X11 window.\n\
    The server prints its WAYLAND_DISPLAY value on stdout.\n\
    Does not start games or implement DRM/KMS or gamepad/touch input.";

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    socket_name: Option<String>,
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>, String> {
    let mut args = args.into_iter();
    let mut options = Options::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--socket" if options.socket_name.is_none() => {
                let name = args.next().ok_or("--socket requires a name")?;
                if name.is_empty() || name.contains('/') || name == "." || name == ".." {
                    return Err("--socket must be a basename, not a path".into());
                }
                options.socket_name = Some(name);
            }
            "--socket" => return Err("--socket may only be specified once".into()),
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    Ok(Some(options))
}


fn run(options: Options) -> Result<(), Box<dyn Error>> {
    let (mut backend, mut host) = winit::init::<GlesRenderer>()?;
    let mut server = Server::new(Mode { size: backend.window_size(), refresh: 60_000 },
        Transform::Flipped180, "pokshell-nested", options.socket_name.as_deref())?;
    let name = server.socket_name();
    println!("WAYLAND_DISPLAY={name}");
    eprintln!("pokcomp: nested compositor listening on {name}");
    loop {
        let status = host.dispatch_new_events(|event| match event {
            WinitEvent::Resized { size, .. } => server.state.resize(Mode { size, refresh: 60_000 }),
            WinitEvent::Input(event) => server.state.input(event),
            _ => {}
        });
        if let PumpStatus::Exit(_) = status { return Ok(()); }
        server.dispatch()?;
        // No game/overlay commit and no host resize: do not allocate,
        // render or submit another GPU framebuffer. Clients schedule their
        // next frame only after the previous presentation callback.
        if !server.dirty() { continue; }
        let size = backend.window_size();
        let damage = Rectangle::from_size(size);
        {
            let (renderer, mut framebuffer) = backend.bind()?;
            let _ = server.state.render(renderer, &mut framebuffer, size, Transform::Flipped180)?;
        }
        backend.submit(Some(&[damage]))?;
        server.rendered();
        server.frame_done();
        server.flush()?;
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let options = match parse_options(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!("{HELP}");
            return Ok(());
        }
        Err(message) => {
            eprintln!("pokcomp: {message}\n{HELP}");
            std::process::exit(2);
        }
    };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "warn".into()))
        .init();
    run(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Options>, String> {
        parse_options(args.iter().map(|x| x.to_string()))
    }

    #[test]
    fn selects_unique_socket_by_default() {
        assert_eq!(parse(&[]).unwrap(), Some(Options::default()));
    }

    #[test]
    fn explicit_socket_name_is_validated() {
        assert_eq!(parse(&["--socket", "pokshell-test"]).unwrap().unwrap().socket_name.as_deref(),
            Some("pokshell-test"));
        assert!(parse(&["--socket"]).is_err());
        assert!(parse(&["--socket", "/tmp/unsafe"]).is_err());
        assert!(parse(&["--socket", "a", "--socket", "b"]).is_err());
    }

    #[test]
    fn unknown_options_do_not_launch_a_server() {
        assert_eq!(parse(&["--help"]).unwrap(), None);
        assert!(parse(&["--unknown"]).is_err());
    }
}
