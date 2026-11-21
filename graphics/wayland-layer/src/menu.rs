// SPDX-License-Identifier: GPL-3.0-or-later
//! Small, deterministic system menu model and software rasterizer.
//! No user-supplied commands, HTML, shell execution or mutable attached buffers.

use font8x8::{BASIC_FONTS, UnicodeFonts};

pub const PANEL_WIDTH: i32 = 320;
const TOP: i32 = 166;
const STEP: i32 = 64;
const ROW_HEIGHT: i32 = 52;
const LEFT: i32 = 16;
const RIGHT: i32 = PANEL_WIDTH - 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Resume,
    RestartGame,
    StopGame,
}

impl MenuAction {
    pub const ALL: [Self; 3] = [Self::Resume, Self::RestartGame, Self::StopGame];

    pub fn label(self) -> &'static str {
        match self {
            Self::Resume => "RESUME",
            Self::RestartGame => "RESTART GAME",
            Self::StopGame => "STOP GAME",
        }
    }

    /// Fixed, auditable pokshelld requests; nothing is interpolated.
    pub fn commands(self) -> &'static [&'static str] {
        match self {
            Self::Resume => &["OVERLAY CLOSE"],
            Self::RestartGame => &["GAME STOP", "GAME START"],
            Self::StopGame => &["GAME STOP"],
        }
    }
}

#[derive(Default)]
pub struct MenuState {
    selected: usize,
}

impl MenuState {
    pub fn selected(&self) -> usize { self.selected }
    pub fn current(&self) -> MenuAction { MenuAction::ALL[self.selected] }
    pub fn reset(&mut self) { self.selected = 0; }
    pub fn next(&mut self) { self.selected = (self.selected + 1) % MenuAction::ALL.len(); }
    pub fn previous(&mut self) {
        self.selected = (self.selected + MenuAction::ALL.len() - 1) % MenuAction::ALL.len();
    }
    pub fn pointer(&mut self, x: f64, y: f64) -> bool {
        let Some(index) = hit_test(x, y) else { return false; };
        let changed = self.selected != index;
        self.selected = index;
        changed
    }
}

pub fn hit_test(x: f64, y: f64) -> Option<usize> {
    if x < f64::from(LEFT) || x >= f64::from(RIGHT) { return None; }
    MenuAction::ALL.iter().enumerate()
        .find(|(i, _)| {
            let top = TOP + *i as i32 * STEP;
            y >= f64::from(top) && y < f64::from(top + ROW_HEIGHT)
        })
        .map(|(i, _)| i)
}

pub fn key_pressed(key: u32, menu: &mut MenuState) -> Option<MenuAction> {
    // Linux evdev keycodes received from wl_keyboard (not X11 keysyms).
    // Arrow keys, Enter and Escape have stable meanings across layouts.
    match key {
        103 | 17 => menu.previous(), // UP or W
        108 | 31 => menu.next(),     // DOWN or S
        28 | 96 | 57 => return Some(menu.current()), // Enter, KP Enter, Space
        1 => return Some(MenuAction::Resume), // Escape
        _ => {}
    }
    None
}

// All colors are BGRA8888 with premultiplied alpha for wl_shm::Argb8888.
const BG: [u8; 4] = [34, 24, 17, 245];
const BOX: [u8; 4] = [69, 47, 34, 250];
const HIGHLIGHT: [u8; 4] = [135, 101, 38, 255];
const ACCENT: [u8; 4] = [196, 175, 61, 255];
const TEXT: [u8; 4] = [249, 246, 243, 255];
const MUTED: [u8; 4] = [181, 177, 167, 255];

fn rect(frame: &mut [u8], width: i32, height: i32, x: i32, y: i32,
    w: i32, h: i32, color: [u8; 4])
{
    let x0 = x.max(0).min(width);
    let x1 = x.saturating_add(w).max(0).min(width);
    let y0 = y.max(0).min(height);
    let y1 = y.saturating_add(h).max(0).min(height);
    for row in y0..y1 {
        for col in x0..x1 {
            let offset = ((row as usize * width as usize) + col as usize) * 4;
            frame[offset..offset + 4].copy_from_slice(&color);
        }
    }
}

fn text(frame: &mut [u8], width: i32, height: i32,
    x: i32, y: i32, value: &str, scale: i32, color: [u8; 4])
{
    for (index, ch) in value.chars().enumerate() {
        if let Some(glyph) = BASIC_FONTS.get(ch) {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8_i32 {
                    if bits & (1_u8 << col) != 0 {
                        rect(frame, width, height,
                            x + index as i32 * 8 * scale + col * scale,
                            y + row as i32 * scale, scale, scale, color);
                    }
                }
            }
        }
    }
}

pub fn paint(width: i32, height: i32, selected: usize) -> Vec<u8> {
    let mut frame = vec![0; width as usize * height as usize * 4];
    for pixel in frame.chunks_exact_mut(4) { pixel.copy_from_slice(&BG); }
    rect(&mut frame, width, height, 0, 0, 4, height, ACCENT);
    text(&mut frame, width, height, 24, 30, "POKSHELL", 2, TEXT);
    text(&mut frame, width, height, 24, 84, "GAME MENU", 2, MUTED);
    rect(&mut frame, width, height, 20, 124, width - 40, 2, ACCENT);

    for (i, item) in MenuAction::ALL.iter().enumerate() {
        let y = TOP + i as i32 * STEP;
        let active = i == selected;
        rect(&mut frame, width, height, LEFT, y, RIGHT - LEFT,
            ROW_HEIGHT, if active { HIGHLIGHT } else { BOX });
        if active {
            rect(&mut frame, width, height, LEFT, y, 5, ROW_HEIGHT, ACCENT);
        }
        text(&mut frame, width, height, 34, y + 17,
            item.label(), 2, if active { TEXT } else { MUTED });
    }
    if height >= 445 {
        rect(&mut frame, width, height, 20, height - 114, width - 40, 1, MUTED);
        text(&mut frame, width, height, 24, height - 91,
            "UP/DOWN  SELECT", 1, MUTED);
        text(&mut frame, width, height, 24, height - 66,
            "ENTER  ESC: BACK", 1, MUTED);
        text(&mut frame, width, height, 24, height - 41,
            "MOUSE: SELECT", 1, MUTED);
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_keyboard_navigation_wraps_and_escape_is_resume() {
        let mut menu = MenuState::default();
        assert_eq!(menu.current(), MenuAction::Resume);
        key_pressed(108, &mut menu);
        assert_eq!(menu.current(), MenuAction::RestartGame);
        key_pressed(108, &mut menu);
        assert_eq!(key_pressed(28, &mut menu), Some(MenuAction::StopGame));
        key_pressed(108, &mut menu);
        assert_eq!(menu.current(), MenuAction::Resume);
        key_pressed(103, &mut menu);
        assert_eq!(menu.current(), MenuAction::StopGame);
        assert_eq!(key_pressed(1, &mut menu), Some(MenuAction::Resume));
        menu.reset();
        assert_eq!(menu.selected(), 0);
    }

    #[test]
    fn pointer_hitboxes_require_a_menu_row_and_panel_x() {
        assert_eq!(hit_test(30., f64::from(TOP + 7)), Some(0));
        assert_eq!(hit_test(30., f64::from(TOP + STEP + 7)), Some(1));
        assert_eq!(hit_test(30., f64::from(TOP + STEP * 2 + 7)), Some(2));
        assert_eq!(hit_test(4., f64::from(TOP + 7)), None);
        assert_eq!(hit_test(30., f64::from(TOP + ROW_HEIGHT + 2)), None);
        let mut menu = MenuState::default();
        assert!(menu.pointer(20., f64::from(TOP + STEP * 2 + 6)));
        assert_eq!(menu.current(), MenuAction::StopGame);
        assert!(!menu.pointer(20., 10.));
        assert_eq!(menu.current(), MenuAction::StopGame);
    }

    #[test]
    fn actions_map_only_to_fixed_ipc_commands() {
        assert_eq!(MenuAction::Resume.commands(), &["OVERLAY CLOSE"]);
        assert_eq!(MenuAction::RestartGame.commands(), &["GAME STOP", "GAME START"]);
        assert_eq!(MenuAction::StopGame.commands(), &["GAME STOP"]);
    }

    #[test]
    fn selected_row_changes_pixels_and_menu_has_real_labels() {
        let a = paint(PANEL_WIDTH, 480, 0);
        let b = paint(PANEL_WIDTH, 480, 1);
        assert_eq!(a.len(), PANEL_WIDTH as usize * 480 * 4);
        assert_ne!(a, b);
        assert!(a.chunks_exact(4).any(|p| p == TEXT));
        assert_eq!(a[3], ACCENT[3]);
    }
}
