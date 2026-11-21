// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure KMS route selection independent of device acquisition or modesetting.
//! The DRM resource handles stay in the caller; this module only ranks
//! candidate connector/encoder/CRTC/mode combinations with stable indices.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub connector: usize,
    pub encoder: usize,
    pub crtc: usize,
    pub mode: usize,
    pub width: u16,
    pub height: u16,
    pub refresh_hz: u32,
    pub currently_routed: bool,
    pub current_mode: bool,
    pub preferred: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteError {
    NoConnectedOutput,
    NoCompatibleCrtc,
}

/// Single-output selection policy. In a later multi-output policy these
/// candidates feed a matcher that prevents two outputs claiming one CRTC.
/// Prefer keeping a valid existing kernel route (less disruption) and then
/// prefer the current mode, then the kernel preferred mode, then resolution/refresh. Stable indices break ties deterministically.
pub fn choose(candidates: &[Candidate], connected_with_modes: bool) -> Result<usize, RouteError> {
    if !connected_with_modes {
        return Err(RouteError::NoConnectedOutput);
    }
    if candidates.is_empty() {
        return Err(RouteError::NoCompatibleCrtc);
    }
    Ok(candidates.iter().enumerate().max_by_key(|(_, c)| (
        u8::from(c.currently_routed),
        u8::from(c.currently_routed && c.current_mode),
        u8::from(c.preferred),
        u32::from(c.width) * u32::from(c.height),
        c.refresh_hz,
        usize::MAX - c.connector,
        usize::MAX - c.encoder,
        usize::MAX - c.crtc,
        usize::MAX - c.mode,
    )).expect("nonempty").0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(connector: usize, encoder: usize, crtc: usize, width: u16, height: u16,
         current: bool) -> Candidate {
        Candidate { connector, encoder, crtc, mode: 0, width, height,
            refresh_hz: 60, currently_routed: current, current_mode: false, preferred: false }
    }

    #[test]
    fn missing_or_disconnected_is_distinct_from_no_route() {
        assert_eq!(choose(&[], false), Err(RouteError::NoConnectedOutput));
        assert_eq!(choose(&[], true), Err(RouteError::NoCompatibleCrtc));
    }
    #[test]
    fn preserves_valid_current_route_before_resolution_upgrade() {
        let list = [
            c(0, 0, 1, 1280, 720, true),
            c(1, 1, 0, 3840, 2160, false),
        ];
        assert_eq!(choose(&list, true), Ok(0));
    }
    #[test]
    fn preferred_and_current_modes_beat_oversized_modes() {
        let mut list = vec![c(0, 0, 0, 5120, 2160, true), c(0, 0, 0, 1280, 800, true),
            c(0, 0, 0, 1920, 1080, true)];
        list[1].preferred = true;
        assert_eq!(choose(&list, true), Ok(1));
        list[2].current_mode = true;
        assert_eq!(choose(&list, true), Ok(2));
    }
    #[test]
    fn selects_best_resolution_and_then_refresh_on_fresh_boot() {
        let mut list = vec![c(0, 0, 0, 1280, 720, false),
            c(1, 1, 1, 1920, 1080, false),
            c(1, 1, 1, 1920, 1080, false)];
        list[2].refresh_hz = 120;
        assert_eq!(choose(&list, true), Ok(2));
    }
    #[test]
    fn deterministic_first_connected_route_on_equal_scores() {
        assert_eq!(choose(&[c(2, 0, 0, 1280, 720, false),
            c(0, 1, 1, 1280, 720, false),
            c(0, 0, 0, 1280, 720, false)], true), Ok(2));
    }
}
