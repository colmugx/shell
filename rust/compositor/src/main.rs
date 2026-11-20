// SPDX-License-Identifier: GPL-3.0-or-later
//! Experimental *nested* Smithay Wayland compositor.
//!
//! Protocol/renderer design adapted from Smithay's MIT-licensed minimal.rs
//! example; see THIRD_PARTY_NOTICES.md. No DRM/KMS or layer-shell support yet.

use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use smithay::{
    backend::{
        input::{InputEvent, InputTime, KeyboardKeyEvent},
        renderer::{
            element::{surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement}, Kind},
            gles::GlesRenderer,
            utils::{draw_render_elements, on_commit_buffer_handler},
            Color32F, Frame, Renderer,
        },
        winit::{self, WinitEvent},
    },
    input::{keyboard::FilterResult, Seat, SeatHandler, SeatState},
    reexports::wayland_server::{protocol::wl_seat, Display},
    utils::{Rectangle, Serial, Transform},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            with_surface_tree_downward, CompositorClientState, CompositorHandler,
            CompositorState, SurfaceAttributes, TraversalAction,
        },
        pointer_constraints::PointerConstraintsHandler,
        selection::{data_device::{DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler}, SelectionHandler},
        shell::xdg::{PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState},
        shm::{ShmHandler, ShmState},
    },
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::{
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::{wl_buffer, wl_surface::{self, WlSurface}},
    Client, ListeningSocket,
};
use winit::event_loop::pump_events::PumpStatus;

const HELP: &str = "pokcomp [--socket NAME]\n\
    Starts a nested Wayland compositor in a host Wayland/X11 window.\n\
    The server prints its WAYLAND_DISPLAY value on stdout.\n\
    Does not start games or implement DRM/KMS, layer-shell or pointer focus.";

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

struct PokCompositor {
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    shm_state: ShmState,
    seat_state: SeatState<Self>,
    data_device_state: DataDeviceState,
    seat: Seat<Self>,
}

impl BufferHandler for PokCompositor {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl XdgShellHandler for PokCompositor {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState { &mut self.xdg_shell_state }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Activated);
        });
        surface.send_configure();
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}
    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
    fn reposition_request(&mut self, _surface: PopupSurface, _positioner: PositionerState, _token: u32) {}
}

impl CompositorHandler for PokCompositor {
    fn compositor_state(&mut self) -> &mut CompositorState { &mut self.compositor_state }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().expect("registered client").compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
    }
}

impl ShmHandler for PokCompositor {
    fn shm_state(&self) -> &ShmState { &self.shm_state }
}

impl SelectionHandler for PokCompositor {
    type SelectionUserData = ();
}
impl DataDeviceHandler for PokCompositor {
    fn data_device_state(&mut self) -> &mut DataDeviceState { &mut self.data_device_state }
}
impl WaylandDndGrabHandler for PokCompositor {}

impl SeatHandler for PokCompositor {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> { &mut self.seat_state }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}
    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: smithay::input::pointer::CursorImageStatus) {}
}
impl PointerConstraintsHandler for PokCompositor {}

#[derive(Default)]
struct ClientState {
    compositor_state: CompositorClientState,
}
impl ClientData for ClientState {
    fn initialized(&self, _id: ClientId) {}
    fn disconnected(&self, _id: ClientId, _reason: DisconnectReason) {}
}

fn send_frame_callbacks(surface: &wl_surface::WlSurface, time_ms: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_, states, &()| {
            for callback in states.cached_state.get::<SurfaceAttributes>().current().frame_callbacks.drain(..) {
                callback.done(time_ms);
            }
        },
        |_, _, &()| true,
    );
}

fn run(options: Options) -> Result<(), Box<dyn Error>> {
    // Initialize the host graphical window BEFORE advertising the nested socket.
    let (mut backend, mut host) = winit::init::<GlesRenderer>()?;

    let mut display: Display<PokCompositor> = Display::new()?;
    let dh = display.handle();
    let mut seat_state = SeatState::new();
    let seat = seat_state.new_wl_seat(&dh, "pokshell-keyboard");

    let mut state = PokCompositor {
        compositor_state: CompositorState::new::<PokCompositor>(&dh),
        xdg_shell_state: XdgShellState::new::<PokCompositor>(&dh),
        shm_state: ShmState::new::<PokCompositor>(&dh, vec![]),
        data_device_state: DataDeviceState::new::<PokCompositor>(&dh),
        seat_state,
        seat,
    };
    let keyboard = state.seat.add_keyboard(Default::default(), 200, 200)?;

    // Use Wayland's lockfile mechanism. Never unlink someone else's socket.
    let listener = match options.socket_name {
        Some(name) => ListeningSocket::bind(name)?,
        None => ListeningSocket::bind_auto("pokshell", 1..128)?,
    };
    let socket_name = listener.socket_name().to_string_lossy().into_owned();
    println!("WAYLAND_DISPLAY={socket_name}");
    eprintln!("pokcomp: nested compositor listening on {socket_name}");
    let start = Instant::now();

    loop {
        let status = host.dispatch_new_events(|event| match event {
            WinitEvent::Input(InputEvent::Keyboard { event }) => {
                keyboard.input::<(), _>(
                    &mut state,
                    event.key_code(),
                    event.state(),
                    0.into(),
                    InputTime::now(),
                    |_, _, _| FilterResult::Forward,
                );
            }
            WinitEvent::Input(InputEvent::PointerMotionAbsolute { .. }) => {
                // Temporary keyboard-only focus policy. Real hit testing,
                // pointer/gamepad/touch routing and overlays are later gates.
                if let Some(surface) = state.xdg_shell_state.toplevel_surfaces().last() {
                    let surface = surface.wl_surface().clone();
                    keyboard.set_focus(&mut state, Some(surface), 0.into());
                }
            }
            _ => {}
        });
        if let PumpStatus::Exit(_) = status {
            return Ok(());
        }

        if let Some(stream) = listener.accept()? {
            display.handle().insert_client(stream, Arc::new(ClientState::default()))?;
        }
        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;

        let size = backend.window_size();
        let damage = Rectangle::from_size(size);
        {
            let (renderer, mut framebuffer) = backend.bind()?;
            // All xdg toplevels currently overlap. This is a protocol and
            // rendering probe, not a full window or surface management policy.
            let elements = state.xdg_shell_state.toplevel_surfaces()
                .iter()
                .flat_map(|surface| {
                    render_elements_from_surface_tree(
                        renderer, surface.wl_surface(), (0, 0), 1.0, 1.0, Kind::Unspecified,
                    )
                })
                .collect::<Vec<WaylandSurfaceRenderElement<GlesRenderer>>>();

            let mut frame = renderer.render(&mut framebuffer, size, Transform::Flipped180)?;
            frame.clear(Color32F::new(0.04, 0.05, 0.07, 1.0), &[damage])?;
            draw_render_elements(&mut frame, 1.0, &elements, &[damage])?;
            let _ = frame.finish()?;

            for surface in state.xdg_shell_state.toplevel_surfaces() {
                send_frame_callbacks(surface.wl_surface(), start.elapsed().as_millis() as u32);
            }
        }
        display.flush_clients()?;
        backend.submit(Some(&[damage]))?;
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

smithay::delegate_dispatch2!(PokCompositor);

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
