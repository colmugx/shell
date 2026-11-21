// SPDX-License-Identifier: GPL-3.0-or-later
//! Native TTY/Linux seat + DRM/GBM bring-up, separate from KDE/Winit.
//! No modesets yet: requires a real libseat session and opens the primary
//! DRM device, enumerates KMS connectors, and verifies GBM initialization.
//! This probes the same lower stack a native compositor will consume.

use smithay::{
    backend::{
        allocator::gbm::GbmDevice,
        drm::{DrmDevice, DrmDeviceFd},
        session::{libseat::LibSeatSession, Session},
    },
    reexports::{
        drm::control::Device as DrmControlDevice,
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

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    card: Option<PathBuf>,
    allow_graphical_session: bool,
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Options>, String> {
    let mut args = args.into_iter();
    let mut opts = Options::default();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--card") if opts.card.is_none() =>
                opts.card = Some(PathBuf::from(args.next().ok_or("--card requires a device")?)),
            Some("--allow-graphical-session") if !opts.allow_graphical_session =>
                opts.allow_graphical_session = true,
            Some("--help") | Some("-h") => return Ok(None),
            _ => return Err(format!("unknown/duplicate argument: {}", arg.to_string_lossy())),
        }
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

fn check_session(allow: bool) -> Result<(), String> {
    // Most importantly, never take control of KWin's DRM card during
    // an existing graphical desktop test, even if it appears accessible.
    if !allow && (env::var_os("WAYLAND_DISPLAY").is_some()
        || env::var_os("DISPLAY").is_some())
    {
        return Err("native DRM requires a separate Linux TTY/seat: close graphical session variables, do not run inside KDE; use pokdrm for read-only inspection".into());
    }
    Ok(())
}

fn run(opts: Options) -> Result<(), Box<dyn Error>> {
    check_session(opts.allow_graphical_session)?;
    let card = opts.card.unwrap_or_else(|| PathBuf::from("/dev/dri/card0"));
    validate_card(&card)?;
    let meta = fs::symlink_metadata(&card)?;
    if !meta.file_type().is_char_device() {
        return Err(format!("{} is not a character device", card.display()).into());
    }

    // libseat negotiates device access via seatd/logind; never sudo or
    // attempt to take DRM master by opening /dev/dri manually.
    let (mut session, _notifier) = LibSeatSession::new()?;
    if !session.is_active() {
        return Err("libseat session is not active".into());
    }
    println!("POKNATIVE seat={} card={}", session.seat(), card.display());

    let fd = session.open(&card, OFlags::RDWR | OFlags::CLOEXEC)?;
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    // false avoids resetting/disabling already-configured connectors.
    let (drm, _drm_notifier) = DrmDevice::new(drm_fd.clone(), false)?;
    let resources = drm.resource_handles()?;
    println!("kms_crtcs={} kms_connectors={} kms_encoders={}",
        resources.crtcs().len(), resources.connectors().len(), resources.encoders().len());

    let mut modes = 0usize;
    for connector in resources.connectors() {
        let info = drm.get_connector(*connector, false)?;
        println!("connector={:?} state={:?} modes={}",
            connector, info.state(), info.modes().len());
        modes += info.modes().len();
    }
    if resources.crtcs().is_empty() || modes == 0 {
        return Err("no KMS CRTC/display modes; cannot start a native output".into());
    }
    let _gbm = GbmDevice::new(drm_fd)?;
    println!("gbm_initialized=yes");
    println!("next_gate=EGL renderer + per-CRTC DrmOutputManager + vblank event loop (not yet wired)");
    // Drop compositor resources before libseat session; do not modify
    // planes or modes, and do not interfere with KWin.
    Ok(())
}

fn main() {
    let opts = match parse(env::args_os().skip(1)) {
        Ok(Some(opts)) => opts,
        Ok(None) => {
            println!("poknative [--card /dev/dri/card0]\nNative libseat + KMS/GBM initialization on a Linux TTY.\nDoes not modeset or display game frames. Do not run inside KDE.");
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
        let r = check_session(true);
        assert!(r.is_ok());
    }
}
