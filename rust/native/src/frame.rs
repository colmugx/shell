// SPDX-License-Identifier: GPL-3.0-or-later
//! Single-output vblank pacing with at most one in-flight KMS buffer.
//! This policy is pure and testable without a DRM card or an active seat.
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub struct FramePump {
    queued: u64,
    presented: u64,
    in_flight_since: Option<Instant>,
}

impl FramePump {
    /// No speculative redraw when the surface tree is unchanged, or while
    /// the previous scanout still owns a framebuffer.
    pub fn should_render(&self, seat_active: bool, dirty: bool) -> bool {
        seat_active && dirty && self.in_flight_since.is_none()
    }

    pub fn has_submitted(&self) -> bool {
        self.queued != 0
    }

    pub fn submitted(&mut self, at: Instant) {
        debug_assert!(self.in_flight_since.is_none());
        self.queued += 1;
        self.in_flight_since = Some(at);
    }

    /// Only a confirmed frame_submitted() event can free the in-flight slot.
    /// Stray or duplicate DRM vblanks do not release or redraw anything.
    pub fn acknowledged(&mut self) -> bool {
        if self.in_flight_since.take().is_some() {
            self.presented += 1;
            true
        } else {
            false
        }
    }

    pub fn timed_out(&self, now: Instant, limit: Duration) -> bool {
        self.in_flight_since.is_some_and(|t| now.saturating_duration_since(t) >= limit)
    }

    pub fn counts(&self) -> (u64, u64) {
        (self.queued, self.presented)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_or_inactive_output_never_renders() {
        let pump = FramePump::default();
        assert!(!pump.should_render(false, true));
        assert!(!pump.should_render(true, false));
        assert!(pump.should_render(true, true));
    }

    #[test]
    fn no_early_reuse_and_no_redraw_after_unchanged_vblank() {
        let mut pump = FramePump::default();
        let now = Instant::now();
        pump.submitted(now);
        assert!(!pump.should_render(true, true));
        assert!(!pump.timed_out(now + Duration::from_millis(300), Duration::from_secs(3)));
        assert!(pump.acknowledged());
        assert!(!pump.acknowledged());
        assert!(!pump.should_render(true, false));
        assert!(pump.should_render(true, true));
        assert_eq!(pump.counts(), (1, 1));
    }

    #[test]
    fn dirty_updates_during_flip_are_coalesced() {
        let mut pump = FramePump::default();
        let now = Instant::now();
        pump.submitted(now);
        assert!(!pump.should_render(true, true)); // never two in flight
        assert!(pump.acknowledged());
        assert!(pump.should_render(true, true)); // latest content can draw
        pump.submitted(now);
        assert_eq!(pump.counts(), (2, 1));
    }

    #[test]
    fn timed_out_page_flip_is_not_acknowledged() {
        let mut pump = FramePump::default();
        let now = Instant::now();
        pump.submitted(now);
        assert!(pump.timed_out(now + Duration::from_secs(4), Duration::from_secs(3)));
        assert_eq!(pump.counts(), (1, 0));
        assert!(!pump.should_render(true, true));
    }
}
