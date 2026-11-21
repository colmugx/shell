// SPDX-License-Identifier: GPL-3.0-or-later
//! XDG-Shell + wl_shm canary: no SDL, GLX, EGL or OpenGL.
use std::{env, error::Error, fs::File, io::{self, Seek, SeekFrom, Write},
    os::{fd::AsFd, unix::{fs::FileTypeExt, net::UnixStream}}, path::{Path, PathBuf}};
use wayland_client::{delegate_noop, globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_buffer, wl_callback, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, QueueHandle};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

fn target(runtime: &Path, name: &str) -> io::Result<PathBuf> {
    if !runtime.is_absolute() || !runtime.is_dir()
        || !name.starts_with("pokshell-") || name.len() <= 9 || name.len() > 70
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "requires absolute XDG_RUNTIME_DIR and pokshell-* WAYLAND_DISPLAY"));
    }
    Ok(runtime.join(name))
}

fn pixels(width: i32, height: i32) -> io::Result<Vec<u8>> {
    if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid XDG dimensions"));
    }
    let len = (width as usize).checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= 48 * 1024 * 1024 && *n <= i32::MAX as usize)
        .ok_or_else(|| io::Error::other("SHM frame exceeds limit"))?;
    let mut data = vec![0; len];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let i = (y * width as usize + x) * 4;
            // XRGB8888, little-endian: B, G, R, unused.
            data[i..i + 4].copy_from_slice(
                if (x / 64 + y / 64) % 2 == 0 { &[40, 160, 55, 255] }
                else { &[50, 75, 190, 255] });
        }
    }
    Ok(data)
}

struct App {
    surface: wl_surface::WlSurface,
    xdg: xdg_surface::XdgSurface,
    shm: wl_shm::WlShm,
    size: (i32, i32),
    submitted: bool,
    closed: bool,
    failure: Option<String>,
    file: Option<File>,
    pool: Option<wl_shm_pool::WlShmPool>,
    buffer: Option<wl_buffer::WlBuffer>,
}

impl App {
    fn submit(&mut self, qh: &QueueHandle<Self>) -> io::Result<()> {
        let (w, h) = (self.size.0.max(1), self.size.1.max(1));
        let data = pixels(w, h)?;
        let mut file = tempfile::tempfile()?;
        file.set_len(data.len() as u64)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&data)?;
        let pool = self.shm.create_pool(file.as_fd(), data.len() as i32, qh, ());
        let buffer = pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Xrgb8888, qh, ());
        self.surface.attach(Some(&buffer), 0, 0);
        self.surface.damage_buffer(0, 0, w, h);
        self.surface.frame(qh, ());
        self.surface.commit();
        self.file = Some(file);
        self.pool = Some(pool);
        self.buffer = Some(buffer);
        self.submitted = true;
        eprintln!("pokxdg-smoke: xdg_shm_buffer_committed size={w}x{h}");
        Ok(())
    }
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

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for App {
    fn event(_: &mut Self, shell: &xdg_wm_base::XdgWmBase, event: xdg_wm_base::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_wm_base::Event::Ping { serial } = event { shell.pong(serial); }
    }
}
impl Dispatch<xdg_toplevel::XdgToplevel, ()> for App {
    fn event(app: &mut Self, _: &xdg_toplevel::XdgToplevel, event: xdg_toplevel::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } =>
                app.size = (if width > 0 {width} else {640}, if height > 0 {height} else {480}),
            xdg_toplevel::Event::Close => app.closed = true,
            _ => (),
        }
    }
}
impl Dispatch<xdg_surface::XdgSurface, ()> for App {
    fn event(app: &mut Self, _: &xdg_surface::XdgSurface, event: xdg_surface::Event,
        _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let xdg_surface::Event::Configure { serial } = event {
            app.xdg.ack_configure(serial);
            eprintln!("pokxdg-smoke: xdg_configured serial={serial}");
            if !app.submitted {
                if let Err(err) = app.submit(qh) {
                    app.failure = Some(err.to_string());
                    app.closed = true;
                }
            }
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for App {
    fn event(_: &mut Self, _: &wl_callback::WlCallback, event: wl_callback::Event,
        _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_callback::Event::Done { callback_data } = event {
            eprintln!("pokxdg-smoke: frame_callback_done time_ms={callback_data}");
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() == 2 && (args[1] == "--help" || args[1] == "-h") {
        println!("pokxdg-smoke: use as poksession --native --game PATH to test XDG/SHM without SDL/EGL");
        return Ok(());
    }
    if args.len() != 1 { return Err("pokxdg-smoke takes no arguments".into()); }
    if env::var_os("WAYLAND_SOCKET").is_some() {
        return Err("WAYLAND_SOCKET overrides the pinned display; refusing".into());
    }
    let runtime = PathBuf::from(env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR missing")?);
    let display = env::var("WAYLAND_DISPLAY")?;
    let path = target(&runtime, &display)?;
    if !path.symlink_metadata()?.file_type().is_socket() {
        return Err("display is not a Unix socket".into());
    }
    let connection = Connection::from_socket(UnixStream::connect(&path)?)?;
    let (globals, mut queue) = registry_queue_init::<App>(&connection)?;
    let qh = queue.handle();
    let compositor: wl_compositor::WlCompositor = globals.bind(&qh, 1..=6, ())?;
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ())?;
    let shell: xdg_wm_base::XdgWmBase = globals.bind(&qh, 1..=6, ())?;
    let surface = compositor.create_surface(&qh, ());
    let xdg = shell.get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_title("pokshell XDG SHM canary".into());
    toplevel.set_app_id("pokxdg-smoke".into());
    let mut app = App {
        surface, xdg, shm, size: (640, 480), submitted: false, closed: false,
        failure: None, file: None, pool: None, buffer: None,
    };
    // Required XDG handshake: bufferless commit -> configure -> ack -> attach.
    app.surface.commit();
    eprintln!("pokxdg-smoke: connected display={display}; waiting_for_xdg_configure");
    while !app.closed { queue.blocking_dispatch(&mut app)?; }
    if let Some(err) = app.failure { return Err(err.into()); }
    Ok(())
}
fn main() {
    if let Err(err) = run() {
        eprintln!("pokxdg-smoke: {err}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_pinned_display() {
        assert!(target(Path::new("/tmp"), "pokshell-7").is_ok());
        for name in ["wayland-0", "pokshell-", "pokshell/x", "../pokshell-1"] {
            assert!(target(Path::new("/tmp"), name).is_err());
        }
        assert!(target(Path::new("relative"), "pokshell-7").is_err());
    }
    #[test]
    fn limits_shm_allocation() {
        assert_eq!(pixels(64, 64).unwrap().len(), 16384);
        for (w,h) in [(0,4),(-1,4),(4097,3),(4096,4096)] {
            assert!(pixels(w,h).is_err());
        }
    }
}
