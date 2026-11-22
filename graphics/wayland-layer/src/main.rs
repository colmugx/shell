// SPDX-License-Identifier: GPL-3.0-or-later
//! Temporary, actual Wayland layer-shell transport: not the final UI or compositor.
//! A separate Wayland game surface continues running underneath this overlay.
mod menu;
mod joystick;

use menu::{MenuAction, MenuState};
use joystick::MenuInput;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;
use wayland_client::{
    delegate_noop,
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_region, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

const PANEL_WIDTH: i32 = 320;
const MAX_HEIGHT: i32 = 4096;


/// Validates a Wayland socket basename without consulting the host KDE
/// WAYLAND_DISPLAY or inherited WAYLAND_SOCKET (which wins over display).
/// This is deliberately a fail-closed game session transport.
fn validate_nested_display(runtime: &Path, name: &str) -> Result<PathBuf, String> {
    if !runtime.is_absolute() {
        return Err("XDG_RUNTIME_DIR must be an absolute path".into());
    }
    if !name.starts_with("pokshell-") || name.len() <= "pokshell-".len()
        || name.len() > 70
        || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("--display requires a pokshell-* Wayland socket basename".into());
    }
    let socket = runtime.join(name);
    let meta = socket.symlink_metadata().map_err(|err| {
        format!("nested Wayland socket {} not found: {err}", socket.display())
    })?;
    if !meta.file_type().is_socket() {
        return Err(format!("nested Wayland endpoint {} is not a socket", socket.display()));
    }
    Ok(socket)
}

fn nested_connection(name: &str) -> Result<Connection, Box<dyn std::error::Error>> {
    let runtime = env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR missing")?;
    let socket = validate_nested_display(Path::new(&runtime), name)?;
    let stream = UnixStream::connect(&socket).map_err(|err| {
        io::Error::new(err.kind(), format!("cannot connect to nested Wayland {}: {err}", socket.display()))
    })?;
    // Crucial: unlike connect_to_env(), from_socket() cannot be silently
    // redirected to KDE/KWin/XWayland by inherited environment variables.
    let connection = Connection::from_socket(stream)?;
    eprintln!("pokshell-layer connected to nested Wayland display={name} path={}", socket.display());
    Ok(connection)
}

fn default_socket() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(|d| PathBuf::from(d).join("pokshell.sock"))
        .unwrap_or_else(|| PathBuf::from("./pokshell.sock"))
}

/// Single subscription; reconnect after daemon restart. No draw requests go
/// through this listener; it only changes a one-bit target surface state.
fn subscribe(path: PathBuf, target: Arc<AtomicBool>) {
    thread::spawn(move || loop {
        target.store(false, Ordering::Release);
        if let Ok(mut connection) = UnixStream::connect(&path) {
            if connection.write_all(b"WATCH\n").is_ok() {
                let mut reader = BufReader::new(connection);
                let mut line = String::new();
                loop {
                    line.clear();
                    if reader.read_line(&mut line).ok().filter(|n| *n > 0).is_none() {
                        break;
                    }
                    if line.starts_with("mode=") {
                        target.store(line.split_whitespace().any(|s| s == "overlay=1"), Ordering::Release);
                    } else if line.contains("type=overlay_changed") {
                        target.store(line.split_whitespace().any(|s| s == "open=1"), Ordering::Release);
                    } else if line.contains("type=game_exited") {
                        target.store(false, Ordering::Release);
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(250));
    });
}

fn paint(width: i32, height: i32, visible: bool) -> Vec<u8> {
    let mut frame = vec![0; width as usize * height as usize * 4];
    if !visible { return frame; }
    for y in 0..height as usize {
        for x in 0..width as usize {
            let p = (y * width as usize + x) * 4;
            // ARGB8888 in native little-endian memory: BGRA, premultiplied.
            let (r,g,b,a) = if x < 3 {
                (72, 172, 186, 245)
            } else if (y >= 52 && y < 54) || (y >= 128 && y < 130) {
                (43, 67, 87, 237)
            } else if y >= 75 && y < 108 && x >= 18 && x < 300 {
                (37, 70, 80, 232)
            } else if y >= 144 && y < 174 && x >= 18 && x < 300 {
                (31, 53, 72, 232)
            } else {
                (18, 30, 43, 237)
            };
            frame[p..p + 4].copy_from_slice(&[b, g, r, a]);
        }
    }
    frame
}

struct App {
    surface: wl_surface::WlSurface,
    layer: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    compositor: wl_compositor::WlCompositor,
    _seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    interactive: bool,
    keyboard_interactive: bool,
    control_socket: PathBuf,
    shm: wl_shm::WlShm,
    pool: Option<wl_shm_pool::WlShmPool>,
    empty: Option<wl_buffer::WlBuffer>,
    panel: Option<wl_buffer::WlBuffer>,
    menu_buffers: Vec<wl_buffer::WlBuffer>,
    backing: Option<File>,
    menu: Option<MenuState>,
    hovered_menu_row: Option<usize>,
    joystick_rx: Option<Receiver<MenuInput>>,
    action_tx: Sender<(MenuAction, Result<(), String>)>,
    action_rx: Receiver<(MenuAction, Result<(), String>)>,
    action_pending: bool,
    configured: bool,
    closed: bool,
    width: i32,
    height: i32,
    drawn: Option<(bool, usize)>,
}

impl App {
    fn allocate(&mut self, qh: &QueueHandle<Self>) -> io::Result<()> {
        let width = self.width;
        let height = self.height;
        if width <= 0 || height <= 0 || height > MAX_HEIGHT {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "unsupported layer dimensions"));
        }
        let size = (width as usize).checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| io::Error::other("framebuffer size overflow"))?;
        // Pre-render immutable selection variants. Never write a wl_shm
        // buffer after a compositor might have attached it.
        let variants = if self.menu.is_some() { MenuAction::ALL.len() } else { 1 };
        let count = variants + 1; // transparent idle buffer
        let total = size.checked_mul(count).ok_or_else(|| io::Error::other("buffer overflow"))?;
        if total > i32::MAX as usize { return Err(io::Error::other("shm allocation exceeds protocol limit")); }
        let mut file = tempfile::tempfile()?;
        file.set_len(total as u64)?;
        for selected in 0..variants {
            file.seek(SeekFrom::Start((size * (selected + 1)) as u64))?;
            let pixels = if self.menu.is_some() {
                menu::paint(width, height, selected)
            } else {
                paint(width, height, true)
            };
            file.write_all(&pixels)?;
        }
        let pool = self.shm.create_pool(file.as_fd(), total as i32, qh, ());
        let empty = pool.create_buffer(0, width, height, width * 4, wl_shm::Format::Argb8888, qh, ());
        let mut buffers = Vec::with_capacity(variants);
        for selected in 0..variants {
            buffers.push(pool.create_buffer((size * (selected + 1)) as i32,
                width, height, width * 4, wl_shm::Format::Argb8888, qh, ()));
        }
        self.pool = Some(pool);
        self.empty = Some(empty);
        if self.menu.is_some() {
            self.panel = None;
            self.menu_buffers = buffers;
        } else {
            self.panel = buffers.pop();
            self.menu_buffers.clear();
        }
        self.backing = Some(file);
        self.drawn = None;
        Ok(())
    }

    fn request_overlay_close(&self, via: &str) {
        let result = UnixStream::connect(&self.control_socket).and_then(|mut sock| {
            sock.set_write_timeout(Some(Duration::from_secs(1)))?;
            sock.write_all(b"OVERLAY CLOSE\n")
        });
        match result {
            Ok(()) => eprintln!("pokshell-layer interactive close requested via={via}"),
            Err(err) => eprintln!("pokshell-layer interactive close failed via={via}: {err}"),
        }
    }

    fn request_menu_action(&mut self, action: MenuAction, via: &'static str) {
        if self.action_pending || self.drawn.map(|x| x.0) != Some(true) { return; }
        self.action_pending = true;
        let socket = self.control_socket.clone();
        let sender = self.action_tx.clone();
        eprintln!("pokshell-layer menu action={} via={via}", action.label());
        // Restart may block waiting for a process group to terminate.
        // Never block the Wayland event loop waiting for pokshelld.
        thread::spawn(move || {
            let result = run_menu_commands(&socket, action);
            let _ = sender.send((action, result));
        });
    }

    fn poll_joystick(&mut self) {
        // The joystick is entirely optional and decoded on a worker. Drain
        // while hidden so stale presses cannot select an action on reopen.
        let mut input = Vec::new();
        if let Some(receiver) = &self.joystick_rx {
            while let Ok(action) = receiver.try_recv() {
                input.push(action);
                if input.len() >= 64 { break; }
            }
        }
        if self.drawn.map(|x| x.0) != Some(true) { return; }
        for action in input {
            let Some(menu) = &mut self.menu else { return; };
            let selected = match action {
                MenuInput::Up => { menu.previous(); None }
                MenuInput::Down => { menu.next(); None }
                MenuInput::Confirm => Some(menu.current()),
                MenuInput::Back => Some(MenuAction::Resume),
            };
            if let Some(selected) = selected {
                self.request_menu_action(selected, "joystick");
            }
        }
    }

    fn poll_action(&mut self) {
        while let Ok((action, result)) = self.action_rx.try_recv() {
            self.action_pending = false;
            match result {
                Ok(()) => eprintln!("pokshell-layer menu completed action={}", action.label()),
                Err(err) => {
                    eprintln!("pokshell-layer menu failed action={}: {err}", action.label());
                }
            }
        }
    }

    fn show(&mut self, visible: bool, qh: &QueueHandle<Self>) {
        if !self.configured { return; }
        let was_visible = self.drawn.map(|(was, _)| was).unwrap_or(false);
        if visible && !was_visible {
            if let Some(menu) = &mut self.menu { menu.reset(); }
            self.action_pending = false;
        }
        if !visible { self.action_pending = false; }
        let selected = self.menu.as_ref().map(MenuState::selected).unwrap_or(0);
        if self.drawn == Some((visible, selected)) { return; }

        // Selection changes only swap immutable pre-rendered wl_shm buffers.
        // Input regions and focus policy change only when visibility does.
        if self.drawn.is_none() || was_visible != visible {
            let region = self.compositor.create_region(qh, ());
            if visible && self.interactive {
                region.add(0, 0, self.width, self.height);
            }
            self.surface.set_input_region(Some(&region));
            region.destroy();
            if self.keyboard_interactive {
                self.layer.set_keyboard_interactivity(if !visible {
                    zwlr_layer_surface_v1::KeyboardInteractivity::None
                } else if self.menu.is_some() {
                    // Game system menu obtains keyboard focus without
                    // requiring a right-click to activate it.
                    zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive
                } else {
                    zwlr_layer_surface_v1::KeyboardInteractivity::OnDemand
                });
            }
        }
        let buffer = if !visible { self.empty.as_ref() }
            else if self.menu.is_some() { self.menu_buffers.get(selected) }
            else { self.panel.as_ref() };
        if let Some(buffer) = buffer {
            self.surface.attach(Some(buffer), 0, 0);
            self.surface.damage_buffer(0, 0, self.width, self.height);
            self.surface.commit();
            self.drawn = Some((visible, selected));
            eprintln!("pokshell-layer buffer committed visible={} selected={}",
                u8::from(visible), selected);
        }
    }
}

fn run_menu_commands(socket: &std::path::Path, action: MenuAction) -> Result<(), String> {
    for request in action.commands() {
        let mut stream = UnixStream::connect(socket).map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(Duration::from_secs(4))).map_err(|e| e.to_string())?;
        stream.set_read_timeout(Some(Duration::from_secs(8))).map_err(|e| e.to_string())?;
        stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
        stream.write_all(b"\n").map_err(|e| e.to_string())?;
        let mut line = String::new();
        BufReader::new(stream.take(1024)).read_line(&mut line).map_err(|e| e.to_string())?;
        if !line.starts_with("OK ") {
            return Err(format!("{request}: {}", line.trim()));
        }
    }
    Ok(())
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for App {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event,
        _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

delegate_noop!(App: wl_compositor::WlCompositor);
delegate_noop!(App: ignore wl_surface::WlSurface);
delegate_noop!(App: ignore wl_shm::WlShm);
delegate_noop!(App: wl_shm_pool::WlShmPool);
delegate_noop!(App: ignore wl_buffer::WlBuffer);
delegate_noop!(App: wl_region::WlRegion);
delegate_noop!(App: zwlr_layer_shell_v1::ZwlrLayerShellV1);

impl Dispatch<wl_seat::WlSeat, ()> for App {
    fn event(state: &mut Self, seat: &wl_seat::WlSeat, event: wl_seat::Event,
        _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            if let Ok(caps) = capabilities.into_result() {
                if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                    state.pointer = Some(seat.get_pointer(qh, ()));
                }
                if state.keyboard_interactive
                    && caps.contains(wl_seat::Capability::Keyboard)
                    && state.keyboard.is_none()
                {
                    state.keyboard = Some(seat.get_keyboard(qh, ()));
                }
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for App {
    fn event(state: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_pointer::Event::Enter { surface_x, surface_y, .. }
            | wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                if state.drawn.map(|x| x.0) == Some(true) {
                    if let Some(menu) = &mut state.menu {
                        state.hovered_menu_row = menu::hit_test(surface_x, surface_y);
                        menu.pointer(surface_x, surface_y);
                    }
                }
            }
            wl_pointer::Event::Button {
                button: 0x110,
                state: wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed), ..
            } if state.interactive && state.drawn.map(|x| x.0) == Some(true) => {
                if let Some(menu) = &state.menu {
                    if state.hovered_menu_row.is_some() {
                        state.request_menu_action(menu.current(), "pointer");
                    }
                } else {
                    state.request_overlay_close("pointer");
                }
            }
            wl_pointer::Event::Leave { .. } => state.hovered_menu_row = None,
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for App {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_keyboard::Event::Enter { .. } = event {
            eprintln!("pokshell-layer keyboard focused");
        }
        if let wl_keyboard::Event::Key {
            key, state: wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed), ..
        } = event {
            if !state.keyboard_interactive || state.drawn.map(|x| x.0) != Some(true) {
                return;
            }
            if let Some(menu) = &mut state.menu {
                if let Some(action) = menu::key_pressed(key, menu) {
                    state.request_menu_action(action, "keyboard");
                }
            } else if key == 1 {
                state.request_overlay_close("escape");
            }
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for App {
    fn event(state: &mut Self, proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width: _, height } => {
                proxy.ack_configure(serial);
                // Width is under our control; compositor chooses output height.
                state.width = PANEL_WIDTH;
                state.height = if height == 0 { 720 } else { height as i32 };
                state.configured = true;
                eprintln!("pokshell-layer configured width={} height={}", state.width, state.height);
                if let Err(err) = state.allocate(qh) {
                    eprintln!("pokshell-layer: allocate: {err}");
                    state.closed = true;
                }
            }
            zwlr_layer_surface_v1::Event::Closed => state.closed = true,
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut socket = default_socket();
    let mut nested_display: Option<String> = None;
    let mut force_visible = false;
    let mut interactive = false;
    let mut keyboard_interactive = false;
    let mut menu_mode = false;
    let mut joystick_path: Option<PathBuf> = None;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--socket") => socket = PathBuf::from(args.next().ok_or("--socket needs an IPC path")?),
            Some("--display") if nested_display.is_none() => {
                nested_display = Some(args.next().ok_or("--display needs a socket basename")?
                    .to_str().ok_or("--display must be UTF-8")?.to_owned());
            },
            Some("--display") => return Err("--display may be specified only once".into()),
            Some("--show") => force_visible = true,
            Some("--interactive") => interactive = true,
            Some("--keyboard-interactive") => {
                keyboard_interactive = true;
                interactive = true;
            },
            Some("--menu") => {
                menu_mode = true;
                keyboard_interactive = true;
                interactive = true;
            },
            Some("--joystick") => {
                joystick_path = Some(PathBuf::from(args.next().ok_or("--joystick needs a path")?));
            },
            Some("--help") => {
                println!("pokshell-layer [--socket IPC_PATH] [--display pokshell-N] [--show] [--interactive] [--keyboard-interactive] [--menu] [--joystick /dev/input/js0]\n--display selects the nested Wayland compositor; --socket selects pokshelld IPC.\n--menu requires --display to prevent menu windows escaping onto KDE.\nRequires Wayland compositor with wlr-layer-shell-v1");
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {}", arg.to_string_lossy()).into()),
        }
    }
    if menu_mode && nested_display.is_none() {
        return Err("--menu requires --display pokshell-N; refusing to render on the KDE desktop".into());
    }
    if joystick_path.is_some() && !menu_mode {
        return Err("--joystick requires --menu".into());
    }
    let joystick_rx = if let Some(path) = joystick_path {
        let (tx, rx) = mpsc::channel();
        joystick::start(path, tx)?;
        Some(rx)
    } else {
        None
    };
    let visible = Arc::new(AtomicBool::new(force_visible));
    if !force_visible { subscribe(socket.clone(), Arc::clone(&visible)); }

    let conn = if let Some(name) = nested_display.as_deref() {
        nested_connection(name)?
    } else {
        // Backwards-compatible diagnostic Layer Shell probe, not a game menu.
        Connection::connect_to_env()?
    };
    let (globals, mut queue) = registry_queue_init::<App>(&conn)?;
    let qh = queue.handle();
    let compositor: wl_compositor::WlCompositor = globals.bind(&qh, 4..=4, ())?;
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ())?;
    let layer_shell: zwlr_layer_shell_v1::ZwlrLayerShellV1 = globals.bind(&qh, 1..=4, ())
        .map_err(|_| "Wayland compositor does not advertise wlr-layer-shell-v1")?;
    let seat = if interactive {
        Some(globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=8, ())?)
    } else {
        None
    };
    let surface = compositor.create_surface(&qh, ());
    // Diagnostic overlay must not steal controller/mouse/keyboard input.
    let empty_region = compositor.create_region(&qh, ());
    surface.set_input_region(Some(&empty_region));
    empty_region.destroy();
    let layer = layer_shell.get_layer_surface(&surface, None,
        zwlr_layer_shell_v1::Layer::Overlay, "pokshell-system".to_owned(), &qh, ());
    layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top |
        zwlr_layer_surface_v1::Anchor::Bottom | zwlr_layer_surface_v1::Anchor::Right);
    layer.set_size(PANEL_WIDTH as u32, 0);
    layer.set_exclusive_zone(-1); // Do not resize the game surface.
    layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None); // Do not steal keyboard focus.
    surface.commit(); // No buffer until first Configure.

    let (action_tx, action_rx) = mpsc::channel();
    let mut app = App {
        surface, layer, compositor, _seat: seat, pointer: None, keyboard: None,
        interactive, keyboard_interactive, control_socket: socket,
        shm, pool: None, empty: None, panel: None, menu_buffers: Vec::new(),
        backing: None, menu: menu_mode.then(MenuState::default), hovered_menu_row: None,
        joystick_rx,
        action_tx, action_rx, action_pending: false,
        configured: false, closed: false, width: PANEL_WIDTH, height: 720, drawn: None,
    };
    while !app.closed {
        queue.roundtrip(&mut app)?;
        app.poll_action();
        app.poll_joystick();
        app.show(visible.load(Ordering::Acquire), &qh);
        thread::sleep(Duration::from_millis(50));
    }
    // Compositor or layer-shell closed our surface; free all resources.
    app.layer.destroy();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_host_wayland_and_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["wayland-0", "wayland-1", "../pokshell-1", "pokshell-", "/tmp/evil",
            "pokshell-a/b", "pokshell-!"] {
            assert!(validate_nested_display(dir.path(), name).is_err(), "{name}");
        }
        assert!(validate_nested_display(Path::new("relative-runtime"), "pokshell-1").is_err());
    }

    #[test]
    fn validates_real_socket_and_rejects_regular_files_and_symlinks() {
        use std::os::unix::net::UnixListener;
        let dir = tempfile::tempdir().unwrap();
        let endpoint = dir.path().join("pokshell-test");
        let _listener = UnixListener::bind(&endpoint).unwrap();
        assert_eq!(validate_nested_display(dir.path(), "pokshell-test").unwrap(), endpoint);
        std::fs::write(dir.path().join("pokshell-file"), "not a socket").unwrap();
        assert!(validate_nested_display(dir.path(), "pokshell-file").is_err());
        std::os::unix::fs::symlink(&endpoint, dir.path().join("pokshell-link")).unwrap();
        assert!(validate_nested_display(dir.path(), "pokshell-link").is_err());
        assert!(validate_nested_display(dir.path(), "pokshell-missing").is_err());
    }

    #[test]
    fn opaque_panel_and_fully_transparent_idle_surface() {
        let transparent = paint(32, 64, false);
        let panel = paint(32, 64, true);
        assert!(transparent.iter().all(|b| *b == 0));
        assert_eq!(panel.len(), 32 * 64 * 4);
        assert!(panel.chunks_exact(4).any(|p| p[3] > 0));
    }

    #[test]
    fn menu_restart_sends_two_separate_acknowledged_commands() {
        use std::os::unix::net::UnixListener;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pokshell-test.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let mut received = Vec::new();
            for _ in 0..2 {
                let (mut client, _) = listener.accept().unwrap();
                client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut request = String::new();
                BufReader::new(&mut client).read_line(&mut request).unwrap();
                received.push(request);
                client.write_all(b"OK TEST\n").unwrap();
            }
            received
        });
        assert!(run_menu_commands(&path, MenuAction::RestartGame).is_ok());
        assert_eq!(server.join().unwrap(), ["GAME STOP\n", "GAME START\n"]);
    }

    #[test]
    fn menu_rejects_error_reply_without_executing_more_actions() {
        use std::os::unix::net::UnixListener;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pokshell-reject.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut client, _) = listener.accept().unwrap();
            client.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut request = String::new();
            BufReader::new(&mut client).read_line(&mut request).unwrap();
            client.write_all(b"ERROR simulated\n").unwrap();
            request
        });
        let err = run_menu_commands(&path, MenuAction::RestartGame).unwrap_err();
        assert!(err.contains("simulated"), "{err}");
        assert_eq!(server.join().unwrap(), "GAME STOP\n");
    }
}
