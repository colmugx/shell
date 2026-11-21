// SPDX-License-Identifier: GPL-3.0-or-later
//! Defensive native output submission gate, independent of the DRM handles.
//! A seat reactivation or hotplug always invalidates the previous scanout
//! generation. The eventual page-flip path must consult may_submit() before
//! touching a possibly stale GPU allocation.

#[derive(Debug, Default)]
pub struct Gate {
    active: bool,
    output_generation: u64,
    configured_generation: Option<u64>,
}

impl Gate {
    pub fn new(active: bool) -> Self {
        Self { active, output_generation: 0, configured_generation: None }
    }
    pub fn active(&self) -> bool { self.active }
    pub fn generation(&self) -> u64 { self.output_generation }
    pub fn may_submit(&self) -> bool {
        self.active && self.configured_generation == Some(self.output_generation)
    }
    pub fn configured(&mut self) {
        if self.active { self.configured_generation = Some(self.output_generation); }
    }
    pub fn pause(&mut self) {
        self.active = false;
        self.invalidate();
    }
    pub fn activate(&mut self) {
        self.active = true;
        self.invalidate();
    }
    pub fn hotplug(&mut self) {
        self.invalidate();
    }
    fn invalidate(&mut self) {
        self.configured_generation = None;
        self.output_generation = self.output_generation.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cannot_submit_until_configured_after_initial_grant() {
        let mut gate = Gate::new(true);
        assert!(!gate.may_submit());
        gate.configured();
        assert!(gate.may_submit());
    }

    #[test]
    fn pause_invalidates_old_scanout_even_after_reactivation() {
        let mut gate = Gate::new(true);
        gate.configured();
        gate.pause();
        assert!(!gate.may_submit());
        let generation = gate.generation();
        gate.configured(); // ignored while inactive
        assert!(!gate.may_submit());
        gate.activate();
        assert_ne!(generation, gate.generation());
        assert!(!gate.may_submit());
        gate.configured();
        assert!(gate.may_submit());
    }

    #[test]
    fn hotplug_requires_full_output_reconfiguration() {
        let mut gate = Gate::new(true);
        gate.configured();
        gate.hotplug();
        assert!(!gate.may_submit());
        gate.configured();
        assert!(gate.may_submit());
    }

    #[test]
    fn inactive_session_cannot_mark_configured() {
        let mut gate = Gate::new(false);
        gate.configured();
        assert!(!gate.may_submit());
        gate.activate();
        assert!(!gate.may_submit());
    }
}
