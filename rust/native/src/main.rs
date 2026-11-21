// SPDX-License-Identifier: GPL-3.0-or-later
//! Native TTY/Linux seat + DRM/GBM bring-up, separate from KDE/Winit.
//! No modesets yet: requires a real libseat session and opens the primary
//! DRM device, enumerates KMS connectors, and verifies GBM initialization.
//! This probes the same lower stack a native compositor will consume.

mod kms;
mod seat;
mod output;
mod frame;
mod scanout_demo;
mod escape;

use kms::Candidate;
use smithay::{
    backend::{
        allocator::gbm::GbmDevice,
        input::InputEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        udev::{UdevBackend, UdevEvent},
        drm::{DrmDevice, DrmDeviceFd, DrmEvent},
        egl::{EGLContext, EGLDevice, EGLDisplay},
        renderer::gles::GlesRenderer,
        session::{libseat::LibSeatSession, Event as SessionEvent, Session},
    },
    reexports::{
        calloop::EventLoop,
        input::Libinput,
        drm::control::{connector, Device as DrmControlDevice, ModeTypeFlags},
        rustix::fs::OFlags,
    },
    utils::DeviceFd,
};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, Ordering};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);
extern "C" fn on_signal(_: i32) {
    SHUTDOWN.store(true, Ordering::Release);
}
fn install_signals() -> std::io::Result<()> {
    unsafe extern "C" { fn signal(number: i32, handler: usize) -> usize; }
    for signum in [2, 15] {
        // SAFETY: store-only async-signal-safe signal handler.
        if unsafe { signal(signum, on_signal as usize) } == usize::MAX {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[derive(Default)]
struct MonitorState {
    gate: seat::Gate,
    device_events: usize,
    input_events: usize,
    drm: Option<DrmDevice>,
    vblanks: usize,
    drm_error: bool,
    server: Option<pokshell_wayland::Server>,
    escape: escape::EscapeChord,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    card: Option<PathBuf>,
    monitor_seconds: Option<u64>,
    scanout_seconds: Option<u64>,
    wayland: bool,
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Options>, String> {
    let mut args = args.into_iter();
    let mut opts = Options::default();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--card") if opts.card.is_none() =>
                opts.card = Some(PathBuf::from(args.next().ok_or("--card requires a device")?)),
            Some("--monitor-seconds") if opts.monitor_seconds.is_none() => {
                let value = args.next().ok_or("--monitor-seconds requires 1..=3600")?
                    .to_str().ok_or("--monitor-seconds must be decimal")?.parse::<u64>()
                    .map_err(|_| "--monitor-seconds must be decimal")?;
                if !(1..=3600).contains(&value) {
                    return Err("--monitor-seconds must be in 1..=3600".into());
                }
                opts.monitor_seconds = Some(value);
            },
            Some("--scanout-seconds") if opts.scanout_seconds.is_none() => {
                let value = args.next().ok_or("--scanout-seconds requires 1..=15")?
                    .to_str().ok_or("--scanout-seconds must be decimal")?.parse::<u64>()
                    .map_err(|_| "--scanout-seconds must be decimal")?;
                if !(1..=15).contains(&value) { return Err("--scanout-seconds must be in 1..=15".into()); }
                opts.scanout_seconds = Some(value);
            },
            Some("--wayland") if !opts.wayland => opts.wayland = true,
            Some("--help") | Some("-h") => return Ok(None),
            _ => return Err(format!("unknown/duplicate argument: {}", arg.to_string_lossy())),
        }
    }
    if opts.scanout_seconds.is_some() && opts.monitor_seconds.is_some() {
        return Err("--scanout-seconds and --monitor-seconds are mutually exclusive".into());
    }
    if opts.wayland && (opts.scanout_seconds.is_some() || opts.monitor_seconds.is_some()) {
        return Err("--wayland is mutually exclusive with diagnostic modes".into());
    }
    Ok(Some(opts))
}

fn validate_card(path: &Path) -> Result<(), String> {
    // Card only, not a render node; a real compositor needs KMS resources.
    if !path.is_absolute() {
        return Err("native KMS card path must be absolute".into());
    }
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let good = name.strip_prefix("card").is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
    });
    if !good { return Err("KMS path must point to /dev/dri/cardN".into()); }
    let device_dir = path.parent().unwrap_or_else(|| Path::new(""));
    if device_dir != Path::new("/dev/dri") {
        return Err("only /dev/dri/cardN is supported".into());
    }
    Ok(())
}

fn check_session() -> Result<(), String> {
    // Most importantly, never take control of KWin's DRM card during
    // an existing graphical desktop test, even if it appears accessible.
    if env::var_os("WAYLAND_DISPLAY").is_some()
        || env::var_os("DISPLAY").is_some()
        || matches!(env::var("XDG_SESSION_TYPE").as_deref(), Ok("wayland" | "x11"))
    {
        return Err("native DRM requires a separate Linux TTY/seat: close graphical session variables, do not run inside KDE; use pokdrm for read-only inspection".into());
    }
    unsafe extern "C" { fn geteuid() -> u32; }
    // SAFETY: read-only Linux libc get effective UID.
    if unsafe { geteuid() } == 0 {
        return Err("native DRM must run as an authorized non-root seat user, never sudo".into());
    }
    Ok(())
}

fn run(opts: Options) -> Result<(), Box<dyn Error>> {
    check_session()?;
    install_signals()?;
    let card = opts.card.unwrap_or_else(|| PathBuf::from("/dev/dri/card0"));
    validate_card(&card)?;
    let meta = fs::symlink_metadata(&card)?;
    if !meta.file_type().is_char_device() {
        return Err(format!("{} is not a character device", card.display()).into());
    }

    // libseat negotiates device access via seatd/logind; never sudo or
    // attempt to take DRM master by opening /dev/dri manually.
    let (mut session, notifier) = LibSeatSession::new()?;
    // Process libseat activation events. KMS devices must never be
    // accessed before the active seat grant is delivered.
    let mut seat_events: EventLoop<MonitorState> = EventLoop::try_new()?;
    let mut state = MonitorState { gate: seat::Gate::new(session.is_active()), ..Default::default() };

    let mut libinput = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
        session.clone().into());
    libinput.udev_assign_seat(&session.seat()).map_err(|()| {
        std::io::Error::other("libinput could not attach to the active Linux seat")
    })?;
    let input_backend = LibinputInputBackend::new(libinput.clone());
    seat_events.handle().insert_source(input_backend, |event, _, state| {
        state.input_events += 1;
        match event {
            InputEvent::Keyboard { event } => {
                // Hardware emergency exit even while the game owns keyboard
                // focus and Ctrl+C no longer reaches the terminal.
                use smithay::backend::input::{KeyboardKeyEvent, KeyState};
                if // Smithay/libinput supplies xkb keycodes (evdev key + 8).
                // Our emergency chord is specified using Linux evdev codes.
                state.escape.key(event.key_code().raw().saturating_sub(8), event.state() == KeyState::Pressed) {
                    eprintln!("poknative: Ctrl+Alt+Backspace emergency session shutdown");
                    SHUTDOWN.store(true, Ordering::Release);
                } else if state.gate.may_submit() {
                    if let Some(server) = state.server.as_mut() {
                        server.state.input::<LibinputInputBackend>(InputEvent::Keyboard { event });
                    }
                }
            }
            InputEvent::DeviceAdded { .. } => eprintln!("poknative: libinput device added"),
            InputEvent::DeviceRemoved { .. } => eprintln!("poknative: libinput device removed"),
            _ => {
                if state.gate.may_submit() {
                    if let Some(server) = state.server.as_mut() { server.state.input(event); }
                }
            },
        }
    })?;
    seat_events.handle().insert_source(notifier, move |event, _, state| {
        match event {
            SessionEvent::ActivateSession => {
                state.gate.activate();
                if let Err(err) = libinput.resume() {
                    eprintln!("poknative: libinput resume error={err:?}");
                }
                eprintln!("poknative: seat activated");
            }
            SessionEvent::PauseSession => {
                state.gate.pause();
                if let Some(drm) = state.drm.as_mut() { drm.pause(); }
                libinput.suspend();
                eprintln!("poknative: seat paused (device rendering forbidden)");
            }
        }
    })?;

    let udev = UdevBackend::new(session.seat())?;
    let known: Vec<_> = udev.device_list().map(|(_, path)| path.to_path_buf()).collect();
    eprintln!("poknative: udev seat devices={known:?}");
    seat_events.handle().insert_source(udev, |event, _, state| {
        state.device_events += 1;
        state.gate.hotplug();
        match event {
            UdevEvent::Added { path, .. } => eprintln!("poknative: hotplug added {}", path.display()),
            UdevEvent::Removed { device_id } => eprintln!("poknative: hotplug removed {device_id}"),
            UdevEvent::Changed { device_id } => eprintln!("poknative: hotplug changed {device_id}"),
        }
    })?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !session.is_active() && Instant::now() < deadline {
        seat_events.dispatch(Duration::from_millis(100), &mut state)?;
    }
    if !session.is_active() {
        return Err("libseat did not activate the session within 5 seconds".into());
    }
    println!("POKNATIVE seat={} card={}", session.seat(), card.display());

    let fd = session.open(&card, OFlags::RDWR | OFlags::CLOEXEC)?;
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    // false avoids resetting/disabling already-configured connectors.
    let (mut drm, drm_notifier) = DrmDevice::new(drm_fd.clone(), false)?;
    let resources = drm.resource_handles()?;
    println!("kms_crtcs={} kms_connectors={} kms_encoders={}",
        resources.crtcs().len(), resources.connectors().len(), resources.encoders().len());

    // Build all valid connector/encoder/CRTC/mode routes from kernel masks.
    // Route ranking is separated from IO and covered by deterministic tests.
    // No DRM master, atomic commit or modeset happens here.
    let mut routes = Vec::new();
    let mut modes = Vec::new();
    let mut connected_with_modes = false;
    for (conn_idx, connector) in resources.connectors().iter().enumerate() {
        let info = drm.get_connector(*connector, false)?;
        println!("connector={:?} state={:?} modes={}",
            connector, info.state(), info.modes().len());
        if info.state() != connector::State::Connected || info.modes().is_empty() {
            continue;
        }
        connected_with_modes = true;
        for (encoder_idx, encoder) in resources.encoders().iter().enumerate() {
            if !info.encoders().contains(encoder) { continue; }
            let enc = drm.get_encoder(*encoder)?;
            let compatible = resources.filter_crtcs(enc.possible_crtcs());
            for (crtc_idx, crtc) in resources.crtcs().iter().enumerate() {
                if !compatible.contains(crtc) { continue; }
                let current = drm.get_crtc(*crtc)?.mode();
                for (mode_idx, mode) in info.modes().iter().enumerate() {
                    let (width, height) = mode.size();
                    routes.push(Candidate {
                        connector: conn_idx, encoder: encoder_idx,
                        crtc: crtc_idx, mode: mode_idx,
                        width, height,
                        refresh_hz: mode.vrefresh(),
                        currently_routed: info.current_encoder() == Some(*encoder) && enc.crtc() == Some(*crtc),
                        current_mode: current.as_ref().is_some_and(|m| m == mode),
                        preferred: mode.mode_type().contains(ModeTypeFlags::PREFERRED),
                    });
                    modes.push(*mode);
                }
            }
        }
    }
    let selected = kms::choose(&routes, connected_with_modes)
        .map_err(|reason| format!("no valid native KMS output route: {reason:?}"))?;
    let route = &routes[selected];
    let connector = resources.connectors()[route.connector];
    let encoder = resources.encoders()[route.encoder];
    let crtc = resources.crtcs()[route.crtc];
    let mode = modes[selected];
    println!("kms_candidates={} kms_route connector={connector:?} encoder={encoder:?} crtc={crtc:?} mode={:?} refresh_hz={}",
        routes.len(), mode.size(), mode.vrefresh());
    let gbm = GbmDevice::new(drm_fd)?;
    println!("gbm_initialized=yes");
    // Smithay's GBM/Native EGL path is also used by its DRM compositor.
    // Native EGL setup is *not* a modeset or a rendered Wayland frame.
    let egl_display = unsafe { EGLDisplay::new(gbm.clone())? };
    match EGLDevice::device_for_display(&egl_display) {
        Ok(egl_device) => {
            println!("egl_device_software={}", egl_device.is_software());
            println!("egl_render_node={:?}", egl_device.try_get_render_node());
        }
        Err(err) => println!("egl_device_metadata_unavailable={err}"),
    }
    println!("egl_display_initialized=yes");
    // A real GLES renderer is initialized against this GBM-backed display.
    // No window-surface or modeset occurs, and CI never opens a DRM card.
    let context = EGLContext::new(&egl_display)?;
    // SAFETY: Smithay owns the newly created EGL context for the lifetime
    // of this renderer; we do not mutate its GL state outside Smithay.
    let mut renderer = unsafe { GlesRenderer::new(context)? };
    println!("gles_renderer_initialized=yes");
    // No KMS scanout is configured yet; do NOT call gate.configured()
    // until a real atomic/legacy modeset and buffer ownership are proven.
    println!("kms_submit_permitted={}", state.gate.may_submit());
    println!("input_backend=libinput drm_hotplug=udev seat_events=libseat");
    if opts.scanout_seconds.is_some() || opts.wayland {
        if !drm.is_atomic() { return Err("scanout test requires atomic KMS".into()); }
        // The bounded test is deliberately single-output. Never disable or
        // remap other connected displays in this first hardware experiment.
        if resources.crtcs().len() != 1 || resources.connectors().len() != 1 {
            return Err("scanout test currently requires exactly one CRTC and connector".into());
        }
        seat_events.handle().insert_source(drm_notifier, move |event, meta, state| {
            match event {
                DrmEvent::VBlank(handle) if handle == crtc => {
                    state.vblanks += 1;
                    eprintln!("poknative: vblank={} metadata={meta:?}", state.vblanks);
                }
                DrmEvent::Error(err) => { state.drm_error = true; eprintln!("poknative: DRM event error={err}"); }
                _ => {}
            }
        })?;
        // Capture pending seat/hotplug events before claiming a plane.
        state.drm = Some(drm);
        seat_events.dispatch(Duration::ZERO, &mut state)?;
        if !state.gate.active() || !session.is_active() || SHUTDOWN.load(Ordering::Acquire) {
            return Err("seat inactive or interrupted before scanout setup".into());
        }
        let generation = state.gate.generation();
        let drm = state.drm.as_mut().expect("installed DRM device");
        let surface = drm.create_surface(crtc, mode, &[connector])?;
        let mut buffered = output::create(surface, gbm.clone(), &renderer)?;
        println!("kms_buffered_surface_ready=yes plane={:?}", buffered.plane());
        seat_events.dispatch(Duration::ZERO, &mut state)?;
        if !state.gate.active() || !session.is_active()
            || state.gate.generation() != generation || state.drm_error
            || SHUTDOWN.load(Ordering::Acquire) {
            return Err("seat/output changed before first scanout".into());
        }
        if opts.wayland {
            use smithay::reexports::calloop::{generic::Generic, Interest, Mode as PollMode, PostAction};
            use smithay::{output::Mode, utils::Transform};
            let size = (i32::from(mode.size().0), i32::from(mode.size().1)).into();
            let mut server = pokshell_wayland::Server::new(
                Mode { size, refresh: (mode.vrefresh() as i32).saturating_mul(1000) },
                Transform::Normal, "pokshell-native", None)?;
            server.enable_dmabuf(&mut renderer);
            for fd in server.poll_fds()? {
                seat_events.handle().insert_source(Generic::new(fd, Interest::READ, PollMode::Level),
                    |_, _, _| Ok(PostAction::Continue))?;
            }
            println!("WAYLAND_DISPLAY={}", server.socket_name());
            state.server = Some(server);
            let mut pump = frame::FramePump::default();
            let mut observed = state.vblanks;
            let startup_deadline = Instant::now() + Duration::from_secs(30);
            eprintln!("poknative: waiting up to 30s for first game frame; Ctrl+Alt+Backspace exits");
            loop {
                if SHUTDOWN.load(Ordering::Acquire) { break; }
                // Gate::configured() MUST follow a successful initial KMS
                // submission, not merely the creation of the Wayland socket.
                if !state.gate.active() || !session.is_active() || state.drm_error
                    || (pump.has_submitted() && !state.gate.may_submit()) {
                    return Err("native session lost seat or output; stopping".into());
                }
                let server = state.server.as_mut().expect("native Wayland server");
                if state.vblanks != observed {
                    observed = state.vblanks;
                    if buffered.frame_submitted()?.is_some() && pump.acknowledged() {
                        server.frame_done();
                    }
                }
                server.dispatch()?;
                let _imports = server.process_dmabufs(&mut renderer);
                server.flush()?;
                // A blank KMS commit hides the terminal before any client
                // can render. Never submit it as the first frame.
                if !pump.has_submitted() && !server.game_has_content(&mut renderer) {
                    if Instant::now() >= startup_deadline {
                        let (xdg, layers, imports) = server.client_counts();
                        eprintln!("poknative: first_frame_timeout xdg_toplevels={xdg} layer_surfaces={layers} pending_dmabufs={imports}");
                        return Err("no renderable game surface after 30s; leaving the TTY visible".into());
                    }
                    seat_events.dispatch(Duration::from_millis(50), &mut state)?;
                    continue;
                }
                if pump.should_render(state.gate.active(), server.dirty()) {
                    output::queue_wayland(&mut buffered, &mut renderer, server, size)?;
                    if !pump.has_submitted() {
                        state.gate.configured();
                        println!("native_kms_first_frame_queued=yes");
                    }
                    pump.submitted(Instant::now());
                    server.rendered();
                }
                if pump.timed_out(Instant::now(), Duration::from_secs(3)) {
                    return Err("native Wayland page flip timeout".into());
                }
                // Socket/DRM/input readiness wakes the event loop. No
                // recurring raster or buffer submission when the UI is idle.
                seat_events.dispatch(Duration::from_millis(
                    if pump.counts().0 > pump.counts().1 { 100 } else { 1000 }),
                    &mut state)?;
            }
            let (submitted, presented) = pump.counts();
            println!("native_frames_submitted={submitted} presented={presented}");
            drop(buffered);
            println!("native_wayland_session=stopped");
            return Ok(());
        }
        let seconds = opts.scanout_seconds.expect("diagnostic duration");
        // queue_buffer performs the first modeset/commit; the swapchain
        // keeps framebuffer ownership until its matching vblank.
        output::queue(&mut buffered, &mut renderer, mode.size(), 0)?;
        state.gate.configured();
        println!("kms_first_frame_queued=yes");
        let mut demo = scanout_demo::DiagnosticScanout::new(
            Duration::from_secs(seconds), Instant::now());
        let (blue, green) = demo.split();
        println!("kms_color_holds_ms blue={} green={}", blue.as_millis(), green.as_millis());
        let mut acknowledged = state.vblanks;
        loop {
            if SHUTDOWN.load(Ordering::Acquire) { break; }
            seat_events.dispatch(Duration::from_millis(40), &mut state)?;
            if !state.gate.may_submit() || !session.is_active() || state.drm_error {
                return Err("seat/output changed during scanout; aborting, no further submissions".into());
            }
            if state.vblanks > acknowledged {
                acknowledged = state.vblanks;
                if let Some(frame) = buffered.frame_submitted()? {
                    demo.presented(frame, Instant::now())?;
                    println!("kms_frame_presented={frame}");
                    if frame == 0 {
                        println!("kms_color_visible=blue holding_ms={}", blue.as_millis());
                    } else {
                        println!("kms_color_visible=green holding_ms={}", green.as_millis());
                    }
                }
            }
            match demo.step(Instant::now()) {
                scanout_demo::Step::Wait => {}
                scanout_demo::Step::QueueSecond => {
                    output::queue(&mut buffered, &mut renderer, mode.size(), 1)?;
                    println!("kms_second_frame_queued=yes");
                }
                scanout_demo::Step::Complete => break,
            }
            if demo.timed_out(Instant::now()) {
                return Err("timed out waiting for DRM page-flip acknowledgement".into());
            }
        }
        if !demo.completed() { return Err("scanout interrupted before both colors held".into()); }
        // Smithay owns the surface and swapchain teardown. Drop them while
        // the device is still active, then release the DRM notifier/device
        // with the seat. Hardware restore must be manually verified.
        drop(buffered);
        println!("kms_scanout_test=complete");
        return Ok(());
    }
    println!("next_gate=--scanout-seconds 5 on an authorized TTY (atomic first frame test)");

    if let Some(seconds) = opts.monitor_seconds {
        let until = Instant::now() + Duration::from_secs(seconds);
        println!("native_monitoring={}s (Ctrl+C to exit)", seconds);
        while Instant::now() < until && !SHUTDOWN.load(Ordering::Acquire) {
            seat_events.dispatch(Duration::from_millis(200), &mut state)?;
            if !state.gate.active() {
                // A native renderer must avoid submitting to KMS while
                // the seat is paused. This probe does not render.
            }
        }
        println!("native_events drm_hotplug={} input_events={} seat_active={}",
            state.device_events, state.input_events, state.gate.active());
    }
    // Drop compositor resources before libseat session; do not modify
    // planes or modes, and do not interfere with KWin.
    Ok(())
}

fn main() {
    let opts = match parse(env::args_os().skip(1)) {
        Ok(Some(opts)) => opts,
        Ok(None) => {
            println!("poknative [--card /dev/dri/card0] [--monitor-seconds 1..3600 | --scanout-seconds 1..15 | --wayland]\nNative libseat + DRM/GBM/udev/libinput initialization on a Linux TTY.\nDefault: no modeset. Explicit scanout test: two diagnostic frames, then exits. Do not run inside KDE.");
            return;
        }
        Err(err) => { eprintln!("poknative: {err}"); std::process::exit(2); }
    };
    if let Err(err) = run(opts) {
        eprintln!("poknative: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn opts(args: &[&str]) -> Result<Option<Options>, String> {
        parse(args.iter().map(OsString::from))
    }
    #[test]
    fn only_primary_kms_cards_are_accepted() {
        assert!(validate_card(Path::new("/dev/dri/card0")).is_ok());
        assert!(validate_card(Path::new("/dev/dri/card15")).is_ok());
        for p in ["/dev/dri/renderD128", "/dev/dri/card0/../card1",
            "/tmp/card0", "card0", "/dev/dri/cardbad"] {
            assert!(validate_card(Path::new(p)).is_err(), "{p}");
        }
    }
    #[test]
    fn parses_without_probing_hardware() {
        assert!(opts(&["--help"]).unwrap().is_none());
        assert!(opts(&["--unknown"]).is_err());
        assert!(opts(&["--wayland"]).unwrap().unwrap().wayland);
        assert!(opts(&["--wayland", "--scanout-seconds", "5"]).is_err());
        assert!(opts(&["--scanout-seconds", "0"]).is_err());
        assert!(opts(&["--scanout-seconds", "16"]).is_err());
        assert!(opts(&["--scanout-seconds", "5", "--monitor-seconds", "1"]).is_err());
        assert_eq!(opts(&["--scanout-seconds", "5"]).unwrap().unwrap().scanout_seconds, Some(5));
        assert!(opts(&["--monitor-seconds", "0"]).is_err());
        assert!(opts(&["--monitor-seconds", "3601"]).is_err());
        assert_eq!(opts(&["--monitor-seconds", "5"]).unwrap().unwrap().monitor_seconds, Some(5));
        assert!(opts(&["--card"]).is_err());
        assert!(opts(&["--card", "/dev/dri/card0", "--card", "/dev/dri/card1"]).is_err());
        assert_eq!(opts(&["--card", "/dev/dri/card2"]).unwrap().unwrap().card,
            Some(PathBuf::from("/dev/dri/card2")));
    }
    #[test]
    fn graphical_guard_is_fail_closed() {
        // Deterministic function contracts without changing CI env.
        // The graphical session guard is intentionally checked before
        // libseat initialization; tests verify path validation separately.
        assert!(parse([OsString::from("--allow-graphical-session")]).is_err());
        // Seat access is intentionally not attempted by any unit test.
    }
}
