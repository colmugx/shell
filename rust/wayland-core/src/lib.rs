// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared Smithay Wayland protocol and composition core.
//!
//! Protocol/renderer design adapted from Smithay's MIT-licensed minimal.rs
//! example; see THIRD_PARTY_NOTICES.md. No DRM/KMS or gamepad input yet.

use std::error::Error;
use std::os::unix::io::OwnedFd;
use std::sync::Arc;
use std::time::Instant;
use std::os::fd::{AsFd, OwnedFd as PollFd};

use smithay::{
    delegate_compositor, delegate_data_device, delegate_output, delegate_seat, delegate_shm, delegate_xdg_shell,
    delegate_layer_shell, delegate_dmabuf,
    backend::{
        input::{AbsolutePositionEvent, ButtonState, Event, InputBackend, InputEvent, KeyboardKeyEvent, PointerButtonEvent, PointerMotionEvent},
        allocator::dmabuf::Dmabuf,
        renderer::{
            ImportDma, ImportEgl,
            element::{surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement}, Kind},
            gles::GlesRenderer,
            utils::{draw_render_elements, on_commit_buffer_handler},
            Color32F, Frame, Renderer,
        },
    },
    desktop::{utils::under_from_surface_tree, WindowSurfaceType},
    input::{
        keyboard::FilterResult,
        pointer::{ButtonEvent, MotionEvent},
        Seat, SeatHandler, SeatState,
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::wayland_server::{protocol::wl_seat, Display},
    utils::{Logical, Physical, Point, Rectangle, Serial, Size, Transform, SERIAL_COUNTER},
    wayland::{
        buffer::BufferHandler,
        output::{OutputHandler, OutputManagerState},
        compositor::{
            with_surface_tree_downward, CompositorClientState, CompositorHandler,
            CompositorState, SurfaceAttributes, TraversalAction,
        },
        selection::{data_device::{ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler}, SelectionHandler},
        shell::{
            wlr_layer::{Layer, LayerSurface, WlrLayerShellHandler, WlrLayerShellState},
            xdg::{PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState},
        },
        shm::{ShmHandler, ShmState},
        dmabuf::{DmabufHandler, DmabufState, DmabufGlobal, ImportNotifier},
    },
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::{
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::{wl_buffer, wl_output::WlOutput, wl_surface::{self, WlSurface}},
    Client, ListeningSocket,
};

const DIAGNOSTIC_LAYER_WIDTH: i32 = 320;

pub struct PokCompositor {
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    layer_shell_state: WlrLayerShellState,
    output_manager_state: OutputManagerState,
    output: Output,
    shm_state: ShmState,
    seat_state: SeatState<Self>,
    data_device_state: DataDeviceState,
    seat: Seat<Self>,
    pointer_position: Point<f64, Logical>,
    dirty: bool,
    seen_game_frame: bool,
    seen_layer_frame: bool,
    dmabuf_state: DmabufState,
    pending_dmabufs: Vec<(Dmabuf, ImportNotifier)>,
}

impl DmabufHandler for PokCompositor {
    fn dmabuf_state(&mut self) -> &mut DmabufState { &mut self.dmabuf_state }

    fn dmabuf_imported(
        &mut self, _global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier,
    ) {
        // Import must be performed by the actual GLES renderer that owns
        // this compositor's EGL context; do not unconditionally acknowledge
        // client buffers or block the Wayland dispatcher on GPU operations.
        const MAX_PENDING_IMPORTS: usize = 128;
        if self.pending_dmabufs.len() >= MAX_PENDING_IMPORTS {
            eprintln!("pokshell: DMA-BUF import queue full, rejecting buffer");
            notifier.failed();
            return;
        }
        self.pending_dmabufs.push((dmabuf, notifier));
    }
}

impl PokCompositor {
    /// The nested host window is pokshell's single output. Wayland XDG
    /// configure sizes are logical pixels (scale 1 in this prototype).
    fn output_extent(&self) -> smithay::utils::Size<i32, Logical> {
        self.output.current_mode()
            .map(|mode| mode.size.to_logical(1))
            .unwrap_or((1280, 720).into())
    }

    /// Send an output-sized configure to a fullscreen/maximized surface.
    /// State *and* size must be updated together so SDL does not render
    /// a small top-left texture after fullscreen requests or host resizes.
    fn configure_fullscreen(&self, surface: &ToplevelSurface, fullscreen: bool) {
        let extent = self.output_extent();
        surface.with_pending_state(|pending| {
            pending.size = Some(extent);
            pending.bounds = Some(extent);
            if fullscreen {
                pending.states.set(xdg_toplevel::State::Fullscreen);
                pending.states.unset(xdg_toplevel::State::Maximized);
            } else {
                pending.states.set(xdg_toplevel::State::Maximized);
                pending.states.unset(xdg_toplevel::State::Fullscreen);
            }
            pending.states.set(xdg_toplevel::State::Activated);
        });
        surface.send_pending_configure();
        eprintln!(
            "pokcomp: xdg {} size={}x{}",
            if fullscreen { "fullscreen" } else { "maximized" },
            extent.w, extent.h
        );
    }

    /// Hit testing uses the surfaces' *actual input regions*. In particular,
    /// the diagnostic overlay has an empty input region and is click-through.
    fn surface_under(
        &self,
        point: Point<f64, Logical>,
        output_size: smithay::utils::Size<i32, smithay::utils::Physical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let x = (output_size.w - DIAGNOSTIC_LAYER_WIDTH).max(0);
        for layer in self.layer_shell_state.layer_surfaces().filter(|layer| layer.alive()).rev() {
            if let Some((surface, origin)) = under_from_surface_tree(
                layer.wl_surface(), point, (x, 0), WindowSurfaceType::ALL,
            ) {
                return Some((surface, origin.to_f64()));
            }
        }
        for game in self.xdg_shell_state.toplevel_surfaces().iter().rev() {
            if let Some((surface, origin)) = under_from_surface_tree(
                game.wl_surface(), point, (0, 0), WindowSurfaceType::ALL,
            ) {
                return Some((surface, origin.to_f64()));
            }
        }
        None
    }
}

impl BufferHandler for PokCompositor {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl XdgShellHandler for PokCompositor {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState { &mut self.xdg_shell_state }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        eprintln!("pokcomp: new xdg_toplevel");
        // The game-first nested session initially fills its sole output.
        // This is XDG maximized (not DRM/KMS or host desktop fullscreen).
        let extent = self.output_extent();
        surface.with_pending_state(|pending| {
            pending.size = Some(extent);
            pending.bounds = Some(extent);
            pending.states.set(xdg_toplevel::State::Maximized);
            pending.states.set(xdg_toplevel::State::Activated);
        });
        surface.send_configure();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, Some(surface.wl_surface().clone()), SERIAL_COUNTER.next_serial());
        }
        eprintln!("pokcomp: xdg initial maximized size={}x{}", extent.w, extent.h);
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<WlOutput>) {
        self.configure_fullscreen(&surface, true);
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        self.configure_fullscreen(&surface, false);
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        self.configure_fullscreen(&surface, false);
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        let extent = self.output_extent();
        surface.with_pending_state(|pending| {
            pending.states.unset(xdg_toplevel::State::Maximized);
            pending.states.unset(xdg_toplevel::State::Fullscreen);
            pending.size = None;
            pending.bounds = Some(extent);
        });
        surface.send_pending_configure();
        eprintln!("pokcomp: xdg restored to normal");
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
        self.dirty = true;
        on_commit_buffer_handler::<Self>(surface);
        // Layer-shell requires the first configure after an empty initial
        // commit. Do not attach a buffer before that configure/ack exchange.
        if let Some(layer) = self.layer_shell_state.layer_surfaces()
            .find(|layer| layer.wl_surface() == surface && layer.alive())
        {
            // The layer client dynamically drops keyboard interactivity on
            // hide. Move focus back to the game before the next key event.
            let wants_keyboard = smithay::wayland::compositor::with_states(surface, |states| {
                states.cached_state.get::<smithay::wayland::shell::wlr_layer::LayerSurfaceCachedState>()
                    .current().keyboard_interactivity
                    != smithay::wayland::shell::wlr_layer::KeyboardInteractivity::None
            });
            if wants_keyboard
                && smithay::wayland::compositor::with_states(surface, |states| {
                    states.cached_state.get::<smithay::wayland::shell::wlr_layer::LayerSurfaceCachedState>()
                        .current().keyboard_interactivity
                        == smithay::wayland::shell::wlr_layer::KeyboardInteractivity::Exclusive
                })
            {
                if let Some(keyboard) = self.seat.get_keyboard() {
                    if keyboard.current_focus().as_ref() != Some(surface) {
                        keyboard.set_focus(self, Some(surface.clone()), SERIAL_COUNTER.next_serial());
                        eprintln!("pokcomp: exclusive system menu keyboard focus");
                    }
                }
            }
            if !wants_keyboard {
                if let Some(keyboard) = self.seat.get_keyboard() {
                    if keyboard.current_focus().as_ref() == Some(surface) {
                        let game = self.xdg_shell_state.toplevel_surfaces().last()
                            .map(|game| game.wl_surface().clone());
                        keyboard.set_focus(self, game, SERIAL_COUNTER.next_serial());
                        eprintln!("pokcomp: keyboard focus returned to game after layer hide");
                    }
                }
            }
            if layer.has_pending_changes() {
                let height = self.output.current_mode().map(|mode| mode.size.h).unwrap_or(720);
                layer.with_pending_state(|state| {
                    state.size = Some((DIAGNOSTIC_LAYER_WIDTH, height).into());
                });
                layer.send_configure();
            }
        }
    }
}

impl WlrLayerShellHandler for PokCompositor {
    fn shell_state(&mut self) -> &mut WlrLayerShellState { &mut self.layer_shell_state }

    fn new_layer_surface(
        &mut self,
        _surface: LayerSurface,
        _output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        // Only this actual Smithay server can confirm it received the layer.
        eprintln!("pokcomp: new layer_surface namespace={namespace}");
        // The initial wl_surface.commit triggers the Configure handshake.
    }
}

impl OutputHandler for PokCompositor {}

impl ShmHandler for PokCompositor {
    fn shm_state(&self) -> &ShmState { &self.shm_state }
}

impl SelectionHandler for PokCompositor {
    type SelectionUserData = ();
}
impl DataDeviceHandler for PokCompositor {
    fn data_device_state(&self) -> &DataDeviceState { &self.data_device_state }
}
impl ClientDndGrabHandler for PokCompositor {}
impl ServerDndGrabHandler for PokCompositor {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

impl SeatHandler for PokCompositor {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> { &mut self.seat_state }

    fn focus_changed(&mut self, _seat: &Seat<Self>, focused: Option<&WlSurface>) {
        eprintln!("pokcomp: keyboard focus {}", if focused.is_some() { "surface" } else { "none" });
    }
    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: smithay::input::pointer::CursorImageStatus) {}
}

#[derive(Default)]
struct ClientState {
    compositor_state: CompositorClientState,
    disconnected: Arc<std::sync::atomic::AtomicBool>,
    dmabuf_enabled: bool,
}
impl ClientData for ClientState {
    fn initialized(&self, _id: ClientId) {}
    fn disconnected(&self, _id: ClientId, _reason: DisconnectReason) { self.disconnected.store(true, std::sync::atomic::Ordering::Release); }
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


impl PokCompositor {
    pub fn resize(&mut self, mode: Mode) {
                let size = mode.size;
                self.output.change_current_state(Some(mode), None, None, None);
                self.output.set_preferred(mode);
                for game in self.xdg_shell_state.toplevel_surfaces() {
                    let previous = game.current_state();
                    let full = previous.states.contains(xdg_toplevel::State::Fullscreen);
                    let max = previous.states.contains(xdg_toplevel::State::Maximized);
                    if full || max {
                        // A resize must reconfigure mapped XDG game clients
                        // as well as the diagnostic layer.
                        let extent = size.to_logical(1);
                        game.with_pending_state(|pending| {
                            pending.size = Some(extent);
                            pending.bounds = Some(extent);
                        });
                        game.send_pending_configure();
                        eprintln!("pokcomp: xdg output resize={}x{}", extent.w, extent.h);
                    }
                }
                for layer in self.layer_shell_state.layer_surfaces().filter(|layer| layer.alive()) {
                    layer.with_pending_state(|pending| {
                        pending.size = Some((DIAGNOSTIC_LAYER_WIDTH, size.h).into());
                    });
                    layer.send_configure();
                }

        self.dirty = true;
    }
    pub fn input<B: InputBackend>(&mut self, input: InputEvent<B>) {
        let Some(keyboard) = self.seat.get_keyboard() else { return; };
        let Some(pointer) = self.seat.get_pointer() else { return; };
        let host_size = self.output.current_mode().expect("configured output").size;
        match input {
            InputEvent::Keyboard { event } => {
                keyboard.input::<(), _>(
                    self,
                    event.key_code(),
                    event.state(),
                    SERIAL_COUNTER.next_serial(),
                    event.time_msec(),
                    |_, _, _| FilterResult::Forward,
                );
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let pos = event.position_transformed(host_size.to_logical(1));
                self.pointer_position = pos;
                let hit = self.surface_under(pos, host_size);
                pointer.motion(self, hit, &MotionEvent {
                    location: pos,
                    serial: SERIAL_COUNTER.next_serial(),
                    time: event.time_msec(),
                });
                pointer.frame(self);
            }
            InputEvent::PointerMotion { event, .. } => {
                let delta = event.delta();
                let pos = Point::from((
                    (self.pointer_position.x + delta.x).clamp(0.0, f64::from((host_size.w - 1).max(0))),
                    (self.pointer_position.y + delta.y).clamp(0.0, f64::from((host_size.h - 1).max(0))),
                ));
                self.pointer_position = pos;
                let hit = self.surface_under(pos, host_size);
                pointer.motion(self, hit, &MotionEvent {
                    location: pos, serial: SERIAL_COUNTER.next_serial(), time: event.time_msec(),
                });
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                if event.state() == ButtonState::Pressed && !pointer.is_grabbed() {
                    let hit = self.surface_under(self.pointer_position, host_size);
                    // Honor the WLR Layer Shell keyboard-interactivity
                    // contract. The new keyboard-enabled diagnostic layer
                    // asks for OnDemand and is focused by click; the classic
                    // pointer-only mode never steals game keyboard focus.
                    let focused = hit.and_then(|(surface, _)| {
                        let is_layer = self.layer_shell_state.layer_surfaces()
                            .any(|layer| layer.alive() && layer.wl_surface() == &surface);
                        if is_layer {
                            let keyboard_interactivity = smithay::wayland::compositor::with_states(&surface, |states| {
                                states.cached_state.get::<smithay::wayland::shell::wlr_layer::LayerSurfaceCachedState>()
                                    .current().keyboard_interactivity
                            });
                            if keyboard_interactivity == smithay::wayland::shell::wlr_layer::KeyboardInteractivity::None {
                                self.xdg_shell_state.toplevel_surfaces().last()
                                    .map(|game| game.wl_surface().clone())
                            } else {
                                Some(surface)
                            }
                        } else {
                            Some(surface)
                        }
                    });
                    keyboard.set_focus(self, focused, serial);
                }
                pointer.button(self, &ButtonEvent {
                    button: event.button_code(),
                    state: event.state(),
                    serial,
                    time: event.time_msec(),
                });
                pointer.frame(self);
            }

            _ => {}
        }
    }
    pub fn render(&mut self, renderer: &mut GlesRenderer,
        framebuffer: &mut smithay::backend::renderer::gles::GlesTarget<'_>,
        size: Size<i32, Physical>, transform: Transform)
        -> Result<smithay::backend::renderer::sync::SyncPoint, Box<dyn Error>>
    {
        let damage = Rectangle::from_size(size);
            // Smithay draws input render elements front-to-back, so layer
            // surfaces must appear before game XDG elements to appear above
            // them in the final composed frame.
            let mut elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
            for layer in self.layer_shell_state.layer_surfaces().filter(|layer| layer.alive()) {
                // The diagnostic client requests TOP|BOTTOM|RIGHT, width=320.
                // General anchor/stacking policy is deliberately not yet present.
                let x = (size.w - DIAGNOSTIC_LAYER_WIDTH).max(0);
                elements.extend(render_elements_from_surface_tree(
                    renderer, layer.wl_surface(), (x, 0), 1.0, 1.0, Kind::Unspecified,
                ));
            }
            // All XDG toplevels overlap at the origin for this experiment.
            let layer_element_count = elements.len();
            for surface in self.xdg_shell_state.toplevel_surfaces() {
                elements.extend(render_elements_from_surface_tree(
                    renderer, surface.wl_surface(), (0, 0), 1.0, 1.0, Kind::Unspecified,
                ));
            }
            let game_element_count = elements.len() - layer_element_count;
            if layer_element_count > 0 && !self.seen_layer_frame {
                eprintln!("pokcomp: layer render elements present");
                self.seen_layer_frame = true;
            }
            if game_element_count > 0 && !self.seen_game_frame {
                eprintln!("pokcomp: game render elements present");
                self.seen_game_frame = true;
            }

            let mut frame = renderer.render(framebuffer, size, transform)?;
            frame.clear(Color32F::new(0.04, 0.05, 0.07, 1.0), &[damage])?;
            draw_render_elements(&mut frame, 1.0, &elements, &[damage])?;
            Ok(frame.finish()?)


    }
}

/// Owns the socket, client protocol state and output-independent composition.
pub struct Server {
    display: Display<PokCompositor>,
    listener: ListeningSocket,
    pub state: PokCompositor,
    start: Instant,
    disconnected: Arc<std::sync::atomic::AtomicBool>,
}

impl Server {
    pub fn new(mode: Mode, transform: Transform, name: &str, socket: Option<&str>)
        -> Result<Self, Box<dyn Error>>
    {
    let mut display: Display<PokCompositor> = Display::new()?;
    let dh = display.handle();
    let mut seat_state = SeatState::new();
    let seat = seat_state.new_wl_seat(&dh, "pokshell-keyboard");

    // Advertise an output: many SDL/Wayland clients require wl_output
    // before they can choose a window/video mode.
    let initial_mode = mode;
    let output = Output::new(name.to_string(), PhysicalProperties {
        size: (0, 0).into(),
        subpixel: Subpixel::Unknown,
        make: "pokshell".into(),
        model: name.into(),
    });
    let _output_global = output.create_global::<PokCompositor>(&dh);
    output.change_current_state(
        Some(initial_mode), Some(transform), None, Some((0, 0).into()),
    );
    output.set_preferred(initial_mode);

    let mut state = PokCompositor {
        compositor_state: CompositorState::new::<PokCompositor>(&dh),
        xdg_shell_state: XdgShellState::new::<PokCompositor>(&dh),
        layer_shell_state: WlrLayerShellState::new::<PokCompositor>(&dh),
        output_manager_state: OutputManagerState::new_with_xdg_output::<PokCompositor>(&dh),
        output,
        shm_state: ShmState::new::<PokCompositor>(&dh, vec![]),
        data_device_state: DataDeviceState::new::<PokCompositor>(&dh),
        seat_state,
        seat,
        pointer_position: (0.0, 0.0).into(),
        dirty: true, seen_game_frame: false, seen_layer_frame: false,
        dmabuf_state: DmabufState::new(),
        pending_dmabufs: Vec::new(),
    };
    state.seat.add_keyboard(Default::default(), 200, 200)?;
    state.seat.add_pointer();


        let listener = match socket {
            Some(name) => ListeningSocket::bind(name)?,
            None => ListeningSocket::bind_auto("pokshell", 1..128)?,
        };
        Ok(Self { display, listener, state, start: Instant::now(),
            disconnected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            dmabuf_enabled: false })
    }

    /// Register a dma-buf global only after the backend's renderer is ready.
    /// EGL on older Mesa/UTM may lack a render node; v3 global + wl_drm
    /// binding supports that case without pretending to have v4 feedback.
    pub fn enable_dmabuf(&mut self, renderer: &mut GlesRenderer) {
        if self.dmabuf_enabled { return; }
        let formats = renderer.dmabuf_formats();
        if formats.is_empty() {
            eprintln!("pokshell: GLES renderer advertises no DMA-BUF formats; SHM only");
            return;
        }
        let _global = self.state.dmabuf_state
            .create_global::<PokCompositor>(&self.display.handle(), formats);
        self.dmabuf_enabled = true;
        eprintln!("pokshell: linux-dmabuf-v1 global registered");
        if let Err(err) = renderer.bind_wl_display(&self.display.handle()) {
            // May be unsupported by the EGL driver. Linux dma-buf remains
            // available for clients which can import it directly.
            eprintln!("pokshell: EGL Wayland display binding unavailable: {err}");
        }
    }

    /// Complete buffer imports on the same renderer used for composition.
    /// A buffer is never acknowledged before the GPU import succeeds.
    pub fn process_dmabufs(&mut self, renderer: &mut GlesRenderer) -> usize {
        let pending = std::mem::take(&mut self.state.pending_dmabufs);
        let total = pending.len();
        for (dmabuf, notifier) in pending {
            match renderer.import_dmabuf(&dmabuf, None) {
                Ok(_texture) => {
                    if notifier.successful::<PokCompositor>().is_err() {
                        eprintln!("pokshell: DMA-BUF client disconnected before import reply");
                    }
                }
                Err(err) => {
                    eprintln!("pokshell: rejecting unimportable DMA-BUF: {err}");
                    notifier.failed();
                }
            }
        }
        total
    }

    pub fn socket_name(&self) -> String {
        self.listener.socket_name().expect("named listener").to_string_lossy().into_owned()
    }
    pub fn poll_fds(&self) -> std::io::Result<[PollFd; 2]> {
        Ok([self.listener.as_fd().try_clone_to_owned()?, self.display.as_fd().try_clone_to_owned()?])
    }
    pub fn dispatch(&mut self) -> Result<(), Box<dyn Error>> {
        while let Some(stream) = self.listener.accept()? {
            self.display.handle().insert_client(stream, Arc::new(ClientState {
                disconnected: self.disconnected.clone(), ..Default::default()
            }))?;
        }
        self.display.dispatch_clients(&mut self.state)?;
        if self.disconnected.swap(false, std::sync::atomic::Ordering::AcqRel) {
            self.state.dirty = true;
        }
        self.flush()
    }
    pub fn flush(&mut self) -> Result<(), Box<dyn Error>> {
        self.display.flush_clients()?;
        Ok(())
    }
    pub fn dirty(&self) -> bool { self.state.dirty }
    pub fn rendered(&mut self) { self.state.dirty = false; }
    pub fn frame_done(&mut self) {
            let timestamp_ms = self.start.elapsed().as_millis() as u32;
            for surface in self.state.xdg_shell_state.toplevel_surfaces() {
                send_frame_callbacks(surface.wl_surface(), timestamp_ms);
            }
            for surface in self.state.layer_shell_state.layer_surfaces().filter(|layer| layer.alive()) {
                send_frame_callbacks(surface.wl_surface(), timestamp_ms);
            }

    }
}
delegate_xdg_shell!(PokCompositor);
delegate_compositor!(PokCompositor);
delegate_shm!(PokCompositor);
delegate_seat!(PokCompositor);
delegate_data_device!(PokCompositor);
delegate_output!(PokCompositor);
delegate_layer_shell!(PokCompositor);
delegate_dmabuf!(PokCompositor);

