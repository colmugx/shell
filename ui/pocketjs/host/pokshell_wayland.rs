// SPDX-License-Identifier: GPL-3.0-or-later
//! Compositable PocketJS UI output for an already-running Wayland server.
//! Implements PocketJS's existing Scanout trait; no DRM, EGL, shell, or root.
use pocket_drivers::{InputClasses, InputFrameBuilder, InputSource, Rect};
use pocket_linux_drm::{Display, Scanout};
use std::{
    env, fs::File, io::{self, Seek, SeekFrom, Write},
    os::{fd::{AsFd, AsRawFd}, unix::{fs::FileTypeExt, net::UnixStream}},
    path::{Path, PathBuf},
    cell::RefCell,
    rc::Rc,
    sync::{atomic::{AtomicU32, Ordering}, Arc},
};
use wayland_client::{
    delegate_noop,
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_registry,
        wl_seat, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

// Physical evdev keys from wl_keyboard::key (XKB keycode = key + 8).
// BTN bits are the published PocketJS v0.17 input contract.
const UP: u32 = 0x0010;
const RIGHT: u32 = 0x0020;
const DOWN: u32 = 0x0040;
const LEFT: u32 = 0x0080;
const TRIANGLE: u32 = 0x1000;
const CIRCLE: u32 = 0x2000;
const CROSS: u32 = 0x4000;
const SHM_LIMIT: usize = 32 * 1024 * 1024;

fn key_button(code: u32) -> u32 {
    match code {
        103 => UP, 106 => RIGHT, 108 => DOWN, 105 => LEFT,
        28 => CIRCLE, // Enter -> focused button
        57 => CROSS,  // Space
        1 | 14 => TRIANGLE, // Escape/Backspace -> Home
        _ => 0,
    }
}

fn explicit_socket(name: &str) -> io::Result<PathBuf> {
    if name.is_empty() || name.len() > 70 ||
        !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "Wayland display must be an explicit socket basename"));
    }
    if env::var_os("WAYLAND_SOCKET").is_some() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "inherited WAYLAND_SOCKET forbidden in explicit Wayland mode"));
    }
    let runtime = PathBuf::from(env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR missing"))?);
    if !runtime.is_absolute() || !runtime.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "XDG_RUNTIME_DIR must be an absolute directory"));
    }
    let path = runtime.join(name);
    if !path.symlink_metadata()?.file_type().is_socket() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "Wayland endpoint is not a socket (symlinks forbidden)"));
    }
    Ok(path)
}

/// Shared keyboard state. A press+release between logical ticks is latched
/// for one sample, avoiding lost button edges on fast virtual/physical keys.
#[derive(Default)]
struct ButtonLatch {
    held: AtomicU32,
    pressed: AtomicU32,
}
impl ButtonLatch {
    fn update(&self, mask: u32, pressed: bool) {
        if pressed {
            self.held.fetch_or(mask, Ordering::Relaxed);
            self.pressed.fetch_or(mask, Ordering::Relaxed);
        } else {
            self.held.fetch_and(!mask, Ordering::Relaxed);
        }
    }
    fn clear(&self) {
        self.held.store(0, Ordering::Relaxed);
        self.pressed.store(0, Ordering::Relaxed);
    }
    fn sample(&self) -> u32 {
        self.held.load(Ordering::Relaxed) | self.pressed.swap(0, Ordering::Relaxed)
    }
}

pub struct WaylandInput {
    connection: Connection,
    queue: Rc<RefCell<EventQueue<State>>>,
    state: Rc<RefCell<State>>,
    buttons: Arc<ButtonLatch>,
}
impl InputSource for WaylandInput {
    fn classes(&self) -> InputClasses { InputClasses::BUTTONS }
    fn poll(&mut self, frame: &mut InputFrameBuilder) {
        // Reading the socket at each logical tick is necessary even when the
        // renderer did not submit a frame. Never block on input: the upstream
        // PocketJS TickSchedule owns the cadence and presentation timing.
        if let Err(err) = poll_wayland(&self.connection, &self.queue, &self.state) {
            eprintln!("pokshell-pocketjs-wayland: keyboard dispatch failed: {err}");
            self.buttons.clear();
            return;
        }
        frame.buttons |= self.buttons.sample();
    }
}

fn poll_wayland(
    conn: &Connection,
    queue: &Rc<RefCell<EventQueue<State>>>,
    state: &Rc<RefCell<State>>,
) -> io::Result<()> {
    conn.flush().map_err(io::Error::other)?;
    // Bound each tick to one nonblocking read so a malicious/very active
    // server cannot starve the Guest's rendering and simulation.
    queue.borrow_mut().dispatch_pending(&mut state.borrow_mut())
        .map_err(io::Error::other)?;
    if let Some(guard) = queue.borrow().prepare_read() {
        let mut fd = libc::pollfd {
            fd: guard.connection_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: fd refers to the live Wayland connection. A zero-timeout
        // poll does not read, mutate or close this fd.
        let ready = unsafe { libc::poll(&mut fd, 1, 0) };
        if ready < 0 { return Err(io::Error::last_os_error()); }
        if ready > 0 {
            guard.read().map_err(io::Error::other)?;
            queue.borrow_mut().dispatch_pending(&mut state.borrow_mut())
                .map_err(io::Error::other)?;
        }
    }
    Ok(())
}

struct State {
    surface: wl_surface::WlSurface,
    xdg: xdg_surface::XdgSurface,
    size: [u32; 2],
    configured: bool,
    closed: bool,
    frame_done: bool,
    released: [bool; 2],
    keyboard: Option<wl_keyboard::WlKeyboard>,
    buttons: Arc<ButtonLatch>,
}
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event,
        _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
delegate_noop!(State: wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: wl_shm_pool::WlShmPool);

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(_: &mut Self, shell: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_wm_base::Event::Ping { serial } = event { shell.pong(serial); }
    }
}
impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(s: &mut Self, _: &xdg_toplevel::XdgToplevel, event: xdg_toplevel::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_toplevel::Event::Close = event { s.closed = true; }
        // The fixed PocketJS 800x600 profile is independent of compositor
        // toplevel resize suggestions; Sway can scale/letterbox this surface.
    }
}
impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(s: &mut Self, _: &xdg_surface::XdgSurface, event: xdg_surface::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_surface::Event::Configure { serial } = event {
            s.xdg.ack_configure(serial);
            s.configured = true;
            eprintln!("pokshell-pocketjs-wayland: xdg_configured serial={serial}");
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(s: &mut Self, _: &wl_callback::WlCallback, event: wl_callback::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_callback::Event::Done { .. } = event { s.frame_done = true; }
    }
}
impl Dispatch<wl_buffer::WlBuffer, usize> for State {
    fn event(s: &mut Self, _: &wl_buffer::WlBuffer, event: wl_buffer::Event,
        slot: &usize, _: &Connection, _: &QueueHandle<Self>) {
        if let wl_buffer::Event::Release = event {
            if let Some(value) = s.released.get_mut(*slot) { *value = true; }
        }
    }
}
impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(s: &mut Self, seat: &wl_seat::WlSeat, event: wl_seat::Event,
        _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let enabled = matches!(capabilities,
                WEnum::Value(mask) if mask.contains(wl_seat::Capability::Keyboard));
            if enabled && s.keyboard.is_none() {
                s.keyboard = Some(seat.get_keyboard(qh, ()));
            } else if !enabled {
                if let Some(keyboard) = s.keyboard.take() { keyboard.release(); }
                s.buttons.clear();
            }
        }
    }
}
impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(s: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Key { key, state, .. } => {
                let mask = key_button(key);
                if mask != 0 {
                    let down = matches!(state, WEnum::Value(wl_keyboard::KeyState::Pressed));
                    s.buttons.update(mask, down);
                    eprintln!("pokshell-pocketjs-wayland: keycode={key} mask={mask:#x} pressed={down}");
                }
            }
            wl_keyboard::Event::Leave { .. } => s.buttons.clear(),
            _ => {}
        }
    }
}

struct Slot {
    file: File,
    _pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
}
pub struct WaylandScanout {
    connection: Connection,
    queue: Rc<RefCell<EventQueue<State>>>,
    state: Rc<RefCell<State>>,
    _toplevel: xdg_toplevel::XdgToplevel,
    _seat: Option<wl_seat::WlSeat>,
    slots: [Slot; 2],
    pending: bool,
    stopped: bool,
    frames: u64,
}
impl WaylandScanout {
    fn dispatch(&mut self) -> io::Result<()> {
        if self.state.borrow().closed { return Err(io::Error::other("Wayland toplevel closed")); }
        self.queue.borrow_mut().blocking_dispatch(&mut self.state.borrow_mut())
            .map_err(io::Error::other)?;
        if self.state.borrow().closed { return Err(io::Error::other("Wayland toplevel closed")); }
        Ok(())
    }
}
impl Scanout for WaylandScanout {
    fn present(&mut self, slot: usize, pixels: &[u8], _damage: &[Rect]) -> io::Result<()> {
        if self.stopped || self.pending || slot >= 2 {
            return Err(io::Error::other("invalid Wayland scanout state"));
        }
        let size = self.state.borrow().size;
        let expected = (size[0] as usize) * (size[1] as usize) * 4;
        if pixels.len() != expected { return Err(io::Error::other("invalid PocketJS pixel size")); }
        while !self.state.borrow().released[slot] { self.dispatch()?; }
        let file = &mut self.slots[slot].file;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(pixels)?;
        file.flush()?;
        {
            let mut state = self.state.borrow_mut();
            state.released[slot] = false;
            state.frame_done = false;
            state.surface.attach(Some(&self.slots[slot].buffer), 0, 0);
            state.surface.damage_buffer(0, 0, size[0] as i32, size[1] as i32);
            state.surface.frame(&self.queue.borrow().handle(), ());
            state.surface.commit();
        }
        self.connection.flush().map_err(io::Error::other)?;
        self.pending = true;
        self.frames += 1;
        if self.frames == 1 {
            eprintln!("pokshell-pocketjs-wayland: xdg_shm_buffer_committed size={}x{} bytes={expected}", size[0], size[1]);
        }
        Ok(())
    }
    fn wait(&mut self) -> io::Result<()> {
        if !self.pending { return Err(io::Error::other("no pending Wayland frame")); }
        while !self.state.borrow().frame_done { self.dispatch()?; }
        self.pending = false;
        Ok(())
    }
    fn stop(&mut self) {
        if self.stopped { return; }
        self.stopped = true;
        // Never owns/disconnects or signals the external compositor.
        eprintln!("pokshell-pocketjs-wayland: stopped frames={} external_compositor_untouched=yes", self.frames);
    }
}
impl Drop for WaylandScanout {
    fn drop(&mut self) { self.stop(); }
}

pub fn open(size: [u32; 2], display: &str) -> io::Result<(Display<WaylandScanout>, WaylandInput)> {
    let n = (size[0] as usize).checked_mul(size[1] as usize)
        .and_then(|v| v.checked_mul(4))
        .filter(|&v| v > 0 && v <= SHM_LIMIT && v <= i32::MAX as usize)
        .ok_or_else(|| io::Error::other("Wayland framebuffer exceeds 32 MiB"))?;
    let path = explicit_socket(display)?;
    let socket = UnixStream::connect(&path)?;
    let connection = Connection::from_socket(socket).map_err(io::Error::other)?;
    let (globals, mut queue) = registry_queue_init::<State>(&connection).map_err(io::Error::other)?;
    let qh = queue.handle();
    let compositor: wl_compositor::WlCompositor =
        globals.bind(&qh, 4..=6, ()).map_err(io::Error::other)?;
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).map_err(io::Error::other)?;
    let shell: xdg_wm_base::XdgWmBase =
        globals.bind(&qh, 1..=6, ()).map_err(io::Error::other)?;
    let seat: Option<wl_seat::WlSeat> =
        globals.bind(&qh, 5..=7, ()).ok();
    let surface = compositor.create_surface(&qh, ());
    let xdg = shell.get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_title("pokshell • PocketJS".into());
    toplevel.set_app_id("org.pokshell.system-ui".into());
    let buttons = Arc::new(ButtonLatch::default());
    let mut state = State {
        surface, xdg, size, configured: false, closed: false,
        frame_done: false, released: [true, true], keyboard: None,
        buttons: buttons.clone(),
    };
    // XDG configure handshake requires the initial commit with no buffer.
    state.surface.commit();
    connection.flush().map_err(io::Error::other)?;
    while !state.configured && !state.closed {
        queue.blocking_dispatch(&mut state).map_err(io::Error::other)?;
    }
    if state.closed { return Err(io::Error::other("Wayland compositor closed toplevel before configure")); }
    let make_slot = |index| -> io::Result<Slot> {
        let file = tempfile::tempfile()?;
        file.set_len(n as u64)?;
        let pool = shm.create_pool(file.as_fd(), n as i32, &qh, ());
        let buffer = pool.create_buffer(
            0, size[0] as i32, size[1] as i32, (size[0] * 4) as i32,
            wl_shm::Format::Argb8888, &qh, index);
        Ok(Slot { file, _pool: pool, buffer })
    };
    let queue = Rc::new(RefCell::new(queue));
    let state = Rc::new(RefCell::new(state));
    let input = WaylandInput {
        connection: connection.clone(),
        queue: Rc::clone(&queue),
        state: Rc::clone(&state),
        buttons,
    };
    let scanout = WaylandScanout {
        connection, queue, state, _toplevel: toplevel, _seat: seat,
        slots: [make_slot(0)?, make_slot(1)?],
        pending: false, stopped: false, frames: 0,
    };
    let output = Display::new(scanout, size)?;
    eprintln!("pokshell-pocketjs-wayland: connected display={display} size={}x{} keyboard=wayland", size[0], size[1]);
    Ok((output, input))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evdev_keyboard_maps_only_navigation_keys() {
        assert_eq!(key_button(103), UP);
        assert_eq!(key_button(106), RIGHT);
        assert_eq!(key_button(108), DOWN);
        assert_eq!(key_button(105), LEFT);
        assert_eq!(key_button(28), CIRCLE);
        assert_eq!(key_button(1), TRIANGLE);
        assert_eq!(key_button(57), CROSS);
        assert_eq!(key_button(40), 0);
    }
    #[test]
    fn rapid_keyboard_taps_are_delivered_once_and_leave_clears_focus() {
        let keys = ButtonLatch::default();
        keys.update(CIRCLE, true);
        keys.update(CIRCLE, false);
        assert_eq!(keys.sample(), CIRCLE, "fast down/up lost between ticks");
        assert_eq!(keys.sample(), 0, "one tap replayed on next tick");
        keys.update(DOWN, true);
        assert_eq!(keys.sample(), DOWN);
        assert_eq!(keys.sample(), DOWN, "held key does not stay pressed");
        keys.clear();
        assert_eq!(keys.sample(), 0, "focus loss must drop stale key state");
    }
    #[test]
    fn rejects_socket_path_escape() {
        for name in ["", "../wayland-1", "wayland/0", "/run/user/1/wayland-0", "wayland 0"] {
            assert!(explicit_socket(name).is_err());
        }
    }
}
