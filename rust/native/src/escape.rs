// SPDX-License-Identifier: GPL-3.0-or-later
//! Emergency TTY escape chord, interpreted from Linux evdev keycodes.
//! It works before the first Wayland surface has rendered, so black-screen
//! sessions are not dependent on a focused game client or VT shortcuts.

#[derive(Debug, Default)]
pub struct EscapeChord {
    left_ctrl: bool,
    right_ctrl: bool,
    left_alt: bool,
    right_alt: bool,
    fired: bool,
}
impl EscapeChord {
    /// Ctrl+Alt+Backspace; return true once per chord press.
    pub fn key(&mut self, code: u32, pressed: bool) -> bool {
        match code {
            29 => self.left_ctrl = pressed,
            97 => self.right_ctrl = pressed,
            56 => self.left_alt = pressed,
            100 => self.right_alt = pressed,
            14 if pressed => {
                if (self.left_ctrl || self.right_ctrl)
                    && (self.left_alt || self.right_alt) && !self.fired
                {
                    self.fired = true;
                    return true;
                }
            }
            14 => self.fired = false,
            _ => {}
        }
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_modifiers_and_a_fresh_backspace_are_required() {
        let mut keys = EscapeChord::default();
        assert!(!keys.key(14, true));
        assert!(!keys.key(29, true));
        assert!(!keys.key(14, false));
        assert!(!keys.key(14, true));
        assert!(!keys.key(56, true));
        assert!(!keys.key(14, false));
        assert!(keys.key(14, true));
        assert!(!keys.key(14, true));
        assert!(!keys.key(14, false));
        assert!(keys.key(14, true));
    }
    #[test]
    fn right_side_modifiers_work_and_release_disarms() {
        let mut keys = EscapeChord::default();
        keys.key(97, true); keys.key(100, true);
        assert!(keys.key(14, true));
        keys.key(14, false); keys.key(100, false);
        assert!(!keys.key(14, true));
    }
    #[test]
    fn missing_modifiers_do_not_exit() {
        let mut keys = EscapeChord::default();
        keys.key(29, true); assert!(!keys.key(14, true));
        keys.key(29, false); keys.key(56, true);
        assert!(!keys.key(14, true));
    }
}
