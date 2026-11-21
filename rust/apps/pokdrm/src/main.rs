// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Linux DRM/KMS preflight. Never modesets, grabs DRM master,
//! opens input devices, modifies a virtual terminal, or runs as root.
//! Establishes the real kernel DRM ABI and records what a native compositor
//! would need before replacing the nested Winit backend.

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

#[repr(C)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
struct DrmModeCardRes {
    fb_id_ptr: u64,
    crtc_id_ptr: u64,
    connector_id_ptr: u64,
    encoder_id_ptr: u64,
    count_fbs: u32,
    count_crtcs: u32,
    count_connectors: u32,
    count_encoders: u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
}

// Linux uapi drm.h: DRM_IOCTL_MODE_GETRESOURCES = DRM_IOWR(0xA0,
// struct drm_mode_card_res). This is read-only and does NOT acquire master.
const DRM_IOCTL_MODE_GETRESOURCES: u64 =
    (3_u64 << 30) | ((std::mem::size_of::<DrmModeCardRes>() as u64) << 16)
    | (b'd' as u64) << 8 | 0xA0;

unsafe extern "C" {
    fn ioctl(fd: i32, request: u64, ...) -> i32;
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    sys_root: PathBuf,
    dev_root: PathBuf,
    strict: bool,
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Options>, String> {
    let mut opts = Options {
        sys_root: PathBuf::from("/sys/class/drm"),
        dev_root: PathBuf::from("/dev/dri"),
        strict: false,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--sys-root") => opts.sys_root = PathBuf::from(args.next().ok_or("--sys-root needs a directory")?),
            Some("--dev-root") => opts.dev_root = PathBuf::from(args.next().ok_or("--dev-root needs a directory")?),
            Some("--strict") if !opts.strict => opts.strict = true,
            Some("--help") | Some("-h") => return Ok(None),
            _ => return Err(format!("unknown argument: {}", arg.to_string_lossy())),
        }
    }
    if !opts.sys_root.is_absolute() || !opts.dev_root.is_absolute() {
        return Err("--sys-root and --dev-root require absolute paths".into());
    }
    Ok(Some(opts))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Connector {
    name: String,
    status: String,
    enabled: String,
    modes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Card {
    name: String,
    device: PathBuf,
    driver: String,
    connectors: Vec<Connector>,
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default().trim().to_owned()
}

fn names(dir: &Path) -> io::Result<Vec<String>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name();
        if let Some(name) = name.to_str() { entries.push(name.to_owned()); }
    }
    entries.sort();
    Ok(entries)
}

fn is_card(name: &str) -> bool {
    name.strip_prefix("card").is_some_and(|digits|
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn scan(sys_root: &Path, dev_root: &Path) -> io::Result<Vec<Card>> {
    let all = names(sys_root)?;
    let mut cards = Vec::new();
    for name in all.iter().filter(|name| is_card(name)) {
        let sys_card = sys_root.join(name);
        let mut connectors = Vec::new();
        for child in all.iter().filter(|child| child.starts_with(&format!("{name}-"))) {
            let path = sys_root.join(child);
            connectors.push(Connector {
                name: child.clone(),
                status: read(&path.join("status")),
                enabled: read(&path.join("enabled")),
                modes: read(&path.join("modes"))
                    .lines().map(str::to_owned).filter(|m| !m.is_empty()).collect(),
            });
        }
        // driver symlink may be unavailable inside containers; no guessing.
        let driver = fs::read_link(sys_card.join("device/driver"))
            .ok().and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "unknown".into());
        cards.push(Card {
            name: name.clone(), device: dev_root.join(name), driver, connectors,
        });
    }
    Ok(cards)
}

fn kms_resources(path: &Path) -> io::Result<DrmModeCardRes> {
    // Opening DRM without O_RDWR or a master request is intentionally
    // non-destructive; a compositor backend must negotiate a seat separately.
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_char_device() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,
            format!("{} is not a DRM character device", path.display())));
    }
    let mut data = DrmModeCardRes::default();
    // SAFETY: the ioctl is a read-only DRM GETRESOURCES query; data is
    // repr(C), sized as the Linux kernel uapi and remains valid during call.
    let rc = unsafe { ioctl(file.as_raw_fd(), DRM_IOCTL_MODE_GETRESOURCES, &mut data) };
    if rc < 0 { return Err(io::Error::last_os_error()); }
    Ok(data)
}

fn run(opts: Options) -> io::Result<bool> {
    println!("POKDRM Linux native display preflight");
    println!("session_type={}", env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unset".into()));
    println!("virtual_terminal={}", env::var("XDG_VTNR").unwrap_or_else(|_| "unset".into()));
    println!("seat={}", env::var("XDG_SEAT").unwrap_or_else(|_| "unset".into()));
    println!("libseat_backend={}", env::var("LIBSEAT_BACKEND").unwrap_or_else(|_| "auto".into()));
    println!("DRM GETRESOURCES ioctl=0x{DRM_IOCTL_MODE_GETRESOURCES:08x}");
    let cards = match scan(&opts.sys_root, &opts.dev_root) {
        Ok(cards) => cards,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            println!("drm_cards=0 (kernel DRM device discovery unavailable)");
            return Ok(false);
        }
        Err(err) => return Err(err),
    };
    println!("drm_cards={}", cards.len());
    let mut usable = false;
    for card in cards {
        println!("card={} driver={} device={}", card.name, card.driver, card.device.display());
        let connected = card.connectors.iter().any(|c| c.status == "connected");
        for connector in &card.connectors {
            println!("connector={} status={} enabled={} modes={}",
                connector.name, connector.status, connector.enabled,
                if connector.modes.is_empty() { "-".into() } else { connector.modes.join(",") });
        }
        match kms_resources(&card.device) {
            Ok(res) => {
                println!("kms_readable=yes crtcs={} connectors={} encoders={} size={}x{}..{}x{}",
                    res.count_crtcs, res.count_connectors, res.count_encoders,
                    res.min_width, res.min_height, res.max_width, res.max_height);
                if res.count_crtcs > 0 && res.count_connectors > 0 && connected {
                    usable = true;
                }
            }
            Err(err) => println!("kms_readable=no error={err}"),
        }
    }
    println!("native_kms_candidate={}", if usable { "yes" } else { "no" });
    if usable {
        println!("next_gate=libseat/session DRM master + GBM/EGL/atomic modeset (NOT IMPLEMENTED)");
    } else {
        println!("next_gate=confirm connected DRM connector, kernel KMS and session device access");
    }
    Ok(usable)
}

fn main() {
    let opts = match parse(env::args_os().skip(1)) {
        Ok(Some(opts)) => opts,
        Ok(None) => {
            println!("pokdrm [--strict] [--sys-root DIR] [--dev-root DIR]\nRead-only Linux DRM/KMS device and connector preflight. --strict fails if no usable KMS candidate.");
            return;
        }
        Err(msg) => { eprintln!("pokdrm: {msg}"); std::process::exit(2); }
    };
    let strict = opts.strict;
    match run(opts) {
        Ok(true) => {}
        Ok(false) if !strict => {}
        Ok(false) => std::process::exit(3),
        Err(err) => { eprintln!("pokdrm: {err}"); std::process::exit(1); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drm_uapi_layout_and_ioctl_match_linux_headers() {
        assert_eq!(std::mem::size_of::<DrmModeCardRes>(), 64);
        assert_eq!(DRM_IOCTL_MODE_GETRESOURCES, 0xc04064a0);
    }

    #[test]
    fn only_card_numeric_names_select_drm_devices() {
        assert!(is_card("card0"));
        assert!(is_card("card21"));
        for wrong in ["card", "card0-HDMI-A-1", "renderD128", "cardevil"] {
            assert!(!is_card(wrong), "{wrong}");
        }
    }

    #[test]
    fn scan_orders_cards_and_connectors_from_fixture() {
        let root = env::temp_dir().join(format!("pokdrm-fixture-{}", std::process::id()));
        let sys = root.join("sys");
        let dev = root.join("dev");
        fs::create_dir_all(&sys).unwrap();
        fs::create_dir_all(&dev).unwrap();
        for name in ["card2", "card0"] { fs::create_dir_all(sys.join(name)).unwrap(); }
        let c = sys.join("card0-HDMI-A-1");
        fs::create_dir_all(&c).unwrap();
        fs::write(c.join("status"), "connected\n").unwrap();
        fs::write(c.join("enabled"), "enabled\n").unwrap();
        fs::write(c.join("modes"), "1920x1080\n1280x720\n").unwrap();
        let cards = scan(&sys, &dev).unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].name, "card0");
        assert_eq!(cards[0].connectors[0].status, "connected");
        assert_eq!(cards[0].connectors[0].modes, ["1920x1080", "1280x720"]);
        assert_eq!(cards[1].name, "card2");
        assert!(cards[1].connectors.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn regular_file_cannot_impersonate_a_kms_device() {
        let root = env::temp_dir().join(format!("pokdrm-not-device-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("card0");
        fs::write(&path, "").unwrap();
        assert_eq!(kms_resources(&path).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cli_validates_strict_flag_and_absolute_paths() {
        assert!(parse([OsString::from("--strict"), OsString::from("--strict")]).is_err());
        assert!(parse([OsString::from("--sys-root"), OsString::from("relative")]).is_err());
        assert!(parse([OsString::from("--unknown")]).is_err());
        assert_eq!(parse([OsString::from("--help")]).unwrap(), None);
        assert!(parse([OsString::from("--strict")]).unwrap().unwrap().strict);
    }
}
