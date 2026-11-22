// SPDX-License-Identifier: GPL-3.0-or-later
//! DRM-free protocol harness for the *native-style* calloop Wayland event path.
//! It NEVER requests libseat, opens /dev/dri or submits a framebuffer.
use std::{error::Error, io, time::{Duration, Instant}};
use pokshell_wayland::Server;
use smithay::{
    output::Mode as VideoMode, utils::Transform,
    reexports::calloop::{EventLoop, generic::Generic, Interest, Mode as PollMode, PostAction},
};
#[derive(Default)]
struct State { server: Option<Server> }
fn run() -> Result<(), Box<dyn Error>> {
    if std::env::args().len() != 1 {
        return Err("pokwayland-protocol takes no arguments; CI-only harness".into());
    }
    let server = Server::new(
        VideoMode {size: (1280, 800).into(), refresh: 60_000},
        Transform::Normal, "pokshell-protocol", Some("pokshell-protocol"),
    )?;
    let mut events: EventLoop<State> = EventLoop::try_new()?;
    for fd in server.poll_fds()? {
        events.handle().insert_source(
            Generic::new(fd, Interest::READ, PollMode::Level),
            |_, _, state| {
                if let Some(server) = state.server.as_mut() {
                    server.dispatch().map_err(|e|
                        io::Error::other(format!("Wayland dispatch: {e}")))?;
                }
                Ok(PostAction::Continue)
            },
        )?;
    }
    println!("WAYLAND_DISPLAY={}", server.socket_name());
    let mut state = State {server: Some(server)};
    let deadline = Instant::now() + Duration::from_secs(18);
    let mut toplevel_reported = false;
    while Instant::now() < deadline {
        events.dispatch(Duration::from_millis(50), &mut state)?;
        // Cover work already queued before FD registration. Neither this
        // nor the calloop callback depends on GPU/DRM readiness.
        let server = state.server.as_mut().expect("server initialized");
        server.dispatch()?;
        if !toplevel_reported && server.client_counts().0 > 0 {
            println!("protocol_harness_xdg_toplevel_seen=yes protocol_callbacks={} backend_dispatch_count={}",
                server.protocol_callbacks(), server.dispatched_requests());
            toplevel_reported = true;
        }
    }
    println!("protocol_harness_dispatched_requests={}",
        state.server.as_ref().unwrap().dispatched_requests());
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("pokwayland-protocol: {e}");
        std::process::exit(1);
    }
}
