// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded KMS diagnostic: hold BOTH colors after their confirmed vblanks.
//! Pure clock state machine; no DRM device access or GPU dependencies.
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step { Wait, QueueSecond, Complete }

#[derive(Debug, Clone, Copy)]
enum Phase {
    AwaitBlue(Instant),
    HoldBlue(Instant),
    AwaitGreen(Instant),
    HoldGreen(Instant),
    Complete,
}

#[derive(Debug)]
pub struct DiagnosticScanout {
    phase: Phase,
    blue_hold: Duration,
    green_hold: Duration,
}

impl DiagnosticScanout {
    pub fn new(total: Duration, now: Instant) -> Self {
        // Durations are measured after the respective frame's presentation.
        let blue_hold = total / 2;
        Self { phase: Phase::AwaitBlue(now), blue_hold, green_hold: total - blue_hold }
    }

    pub fn presented(&mut self, id: u8, now: Instant) -> Result<(), &'static str> {
        self.phase = match (self.phase, id) {
            (Phase::AwaitBlue(_), 0) => Phase::HoldBlue(now),
            (Phase::AwaitGreen(_), 1) => Phase::HoldGreen(now),
            _ => return Err("unexpected or duplicated diagnostic page-flip acknowledgement"),
        };
        Ok(())
    }

    pub fn step(&mut self, now: Instant) -> Step {
        match self.phase {
            Phase::HoldBlue(since) if now.saturating_duration_since(since) >= self.blue_hold => {
                self.phase = Phase::AwaitGreen(now);
                Step::QueueSecond
            }
            Phase::HoldGreen(since) if now.saturating_duration_since(since) >= self.green_hold => {
                self.phase = Phase::Complete;
                Step::Complete
            }
            _ => Step::Wait,
        }
    }

    pub fn timed_out(&self, now: Instant) -> bool {
        const FLIP_TIMEOUT: Duration = Duration::from_secs(3);
        match self.phase {
            Phase::AwaitBlue(since) | Phase::AwaitGreen(since) =>
                now.saturating_duration_since(since) >= FLIP_TIMEOUT,
            _ => false,
        }
    }
    pub fn completed(&self) -> bool { matches!(self.phase, Phase::Complete) }
    pub fn split(&self) -> (Duration, Duration) { (self.blue_hold, self.green_hold) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn five_seconds_hold_both_colors_for_2500_milliseconds() {
        let t = Instant::now();
        let mut demo = DiagnosticScanout::new(Duration::from_secs(5), t);
        assert_eq!(demo.split(), (Duration::from_millis(2500), Duration::from_millis(2500)));
        assert_eq!(demo.step(t), Step::Wait);
        let blue = t + Duration::from_millis(20);
        demo.presented(0, blue).unwrap();
        assert_eq!(demo.step(blue + Duration::from_millis(2499)), Step::Wait);
        assert_eq!(demo.step(blue + Duration::from_millis(2500)), Step::QueueSecond);
        assert_eq!(demo.step(blue + Duration::from_secs(3)), Step::Wait);
        let green = blue + Duration::from_millis(2510);
        demo.presented(1, green).unwrap();
        assert_eq!(demo.step(green + Duration::from_millis(2499)), Step::Wait);
        assert_eq!(demo.step(green + Duration::from_millis(2500)), Step::Complete);
        assert!(demo.completed());
    }
    #[test]
    fn one_second_still_shows_both_colors() {
        let t = Instant::now();
        let mut demo = DiagnosticScanout::new(Duration::from_secs(1), t);
        assert_eq!(demo.split(), (Duration::from_millis(500), Duration::from_millis(500)));
        demo.presented(0, t).unwrap();
        assert_eq!(demo.step(t + Duration::from_millis(499)), Step::Wait);
        assert_eq!(demo.step(t + Duration::from_millis(500)), Step::QueueSecond);
    }
    #[test]
    fn rejects_out_of_order_or_duplicate_acknowledgements() {
        let t = Instant::now();
        let mut demo = DiagnosticScanout::new(Duration::from_secs(5), t);
        assert!(demo.presented(1, t).is_err());
        demo.presented(0, t).unwrap();
        assert!(demo.presented(0, t).is_err());
        assert!(demo.presented(1, t).is_err());
        assert_eq!(demo.step(t + Duration::from_secs(3)), Step::QueueSecond);
        demo.presented(1, t + Duration::from_secs(3)).unwrap();
        assert!(demo.presented(1, t).is_err());
    }
    #[test]
    fn flip_timeout_is_not_a_color_hold_timeout() {
        let t = Instant::now();
        let mut demo = DiagnosticScanout::new(Duration::from_secs(15), t);
        assert!(!demo.timed_out(t + Duration::from_millis(2999)));
        assert!(demo.timed_out(t + Duration::from_secs(3)));
        demo.presented(0, t + Duration::from_millis(10)).unwrap();
        assert!(!demo.timed_out(t + Duration::from_secs(8)));
        assert_eq!(demo.step(t + Duration::from_millis(7510)), Step::QueueSecond);
        assert!(demo.timed_out(t + Duration::from_millis(10510)));
    }
}
