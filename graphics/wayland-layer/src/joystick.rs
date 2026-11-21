// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional Linux joystick API (JS_EVENT_*), kept separate from Wayland.
//! Prototype input only: does not grab devices or suppress events to the game.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;

const JS_EVENT_BUTTON: u8 = 0x01;
const JS_EVENT_AXIS: u8 = 0x02;
const JS_EVENT_INIT: u8 = 0x80;
const AXIS_DEADZONE: i16 = 18_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuInput {
    Up,
    Down,
    Confirm,
    Back,
}

/// Per-axis latch suppresses repeated navigation while a stick is held.
#[derive(Default)]
pub struct JoystickMapper {
    last_axis: HashMap<u8, i8>,
}

impl JoystickMapper {
    pub fn decode(&mut self, raw: [u8; 8]) -> Option<MenuInput> {
        let value = i16::from_le_bytes([raw[4], raw[5]]);
        let kind = raw[6];
        let number = raw[7];
        if kind & JS_EVENT_INIT != 0 { return None; }
        match kind {
            JS_EVENT_BUTTON if value == 1 => match number {
                0 | 7 => Some(MenuInput::Confirm), // typical A / Start
                1 => Some(MenuInput::Back),        // typical B
                _ => None,
            },
            JS_EVENT_AXIS if number == 1 || number == 7 => {
                let direction = if value > AXIS_DEADZONE { 1 }
                    else if value < -AXIS_DEADZONE { -1 } else { 0 };
                let last = self.last_axis.entry(number).or_default();
                if *last == direction { return None; }
                *last = direction;
                match direction {
                    -1 => Some(MenuInput::Up),
                    1 => Some(MenuInput::Down),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

pub fn start(path: PathBuf, sender: Sender<MenuInput>) -> io::Result<()> {
    // Open on the caller's ordinary user credentials. Never sudo/grab an
    // evdev device on behalf of the game. A closed/unplugged device simply
    // terminates this optional input thread.
    let mut device = File::open(&path)?;
    thread::spawn(move || {
        eprintln!("pokshell-layer joystick reading from {}", path.display());
        let mut mapper = JoystickMapper::default();
        loop {
            let mut data = [0_u8; 8];
            if let Err(err) = device.read_exact(&mut data) {
                eprintln!("pokshell-layer joystick stopped: {err}");
                break;
            }
            if let Some(action) = mapper.decode(data) {
                if sender.send(action).is_err() { break; }
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: u8, number: u8, value: i16) -> [u8; 8] {
        let [lo, hi] = value.to_le_bytes();
        [0, 0, 0, 0, lo, hi, kind, number]
    }

    #[test]
    fn axis_moves_once_until_neutral_then_can_repeat() {
        let mut mapper = JoystickMapper::default();
        assert_eq!(mapper.decode(event(JS_EVENT_AXIS, 1, 22_000)), Some(MenuInput::Down));
        assert_eq!(mapper.decode(event(JS_EVENT_AXIS, 1, 22_000)), None);
        assert_eq!(mapper.decode(event(JS_EVENT_AXIS, 1, 100)), None);
        assert_eq!(mapper.decode(event(JS_EVENT_AXIS, 1, -22_000)), Some(MenuInput::Up));
        assert_eq!(mapper.decode(event(JS_EVENT_AXIS, 1, 0)), None);
        assert_eq!(mapper.decode(event(JS_EVENT_AXIS, 1, 22_000)), Some(MenuInput::Down));
    }

    #[test]
    fn maps_common_buttons_and_discards_init_events() {
        let mut mapper = JoystickMapper::default();
        assert_eq!(mapper.decode(event(JS_EVENT_BUTTON, 0, 1)), Some(MenuInput::Confirm));
        assert_eq!(mapper.decode(event(JS_EVENT_BUTTON, 1, 1)), Some(MenuInput::Back));
        assert_eq!(mapper.decode(event(JS_EVENT_BUTTON, 7, 1)), Some(MenuInput::Confirm));
        assert_eq!(mapper.decode(event(JS_EVENT_BUTTON, 0, 0)), None);
        assert_eq!(mapper.decode(event(JS_EVENT_INIT | JS_EVENT_BUTTON, 0, 1)), None);
    }

    #[test]
    fn avoids_interpreting_horizontal_axes_as_menu_navigation() {
        let mut mapper = JoystickMapper::default();
        assert_eq!(mapper.decode(event(JS_EVENT_AXIS, 0, -30_000)), None);
        assert_eq!(mapper.decode(event(JS_EVENT_AXIS, 7, 25_000)), Some(MenuInput::Down));
        assert_eq!(mapper.decode(event(JS_EVENT_AXIS, 7, 25_000)), None);
    }
}
