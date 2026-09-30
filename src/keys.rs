//! G-key → emulated key mapping and the key state machine.
//!
//! The keyboard reports G-key state as a 16-bit mask (bit 0 = G1 … bit 15 = G16).
//! [`Remapper`] turns changes of that mask into key down/up events for the keys in the mapping.

use std::collections::HashMap;

/// A key sent with SendInput. Scan codes are explicit: on non-Japanese layouts
/// MapVirtualKey returns 0 for the Japanese keys, and some games read only scan codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub name: &'static str,
    pub vk: u16,
    pub scan: u16,
}

const fn key(name: &'static str, vk: u16, scan: u16) -> Key {
    Key { name, vk, scan }
}

pub const F13: Key = key("F13", 0x7C, 0x64);
pub const F14: Key = key("F14", 0x7D, 0x65);
pub const F15: Key = key("F15", 0x7E, 0x66);
pub const F16: Key = key("F16", 0x7F, 0x67);
pub const F17: Key = key("F17", 0x80, 0x68);
pub const F18: Key = key("F18", 0x81, 0x69);
pub const F19: Key = key("F19", 0x82, 0x6A);
pub const F20: Key = key("F20", 0x83, 0x6B);
pub const F21: Key = key("F21", 0x84, 0x6C);
pub const F22: Key = key("F22", 0x85, 0x6D);
pub const F23: Key = key("F23", 0x86, 0x6E);
pub const F24: Key = key("F24", 0x87, 0x76);
// Japanese keys: they do nothing on RU/EN layouts but are distinct keys for apps and games
pub const KANA: Key = key("Kana", 0x15, 0x70);
pub const CONVERT: Key = key("Convert", 0x1C, 0x79);
pub const NONCONVERT: Key = key("NonConvert", 0x1D, 0x7B);
// Left-side modifiers for combinations in the mapping: generic VK_CONTROL shows up as both
// Ctrls in key testers. Unused by the default mapping, kept for editing it.
#[allow(dead_code)]
pub const CTRL: Key = key("Ctrl", 0xA2, 0x1D);
#[allow(dead_code)]
pub const SHIFT: Key = key("Shift", 0xA0, 0x2A);
#[allow(dead_code)]
pub const ALT: Key = key("Alt", 0xA4, 0x38);

/// G-key number → key combination (modifiers first). Edit to taste, e.g. `&[CTRL, F13]`.
/// There is no physical G8: its bit exists in the report but is never set.
pub fn mapping(g: u8) -> Option<&'static [Key]> {
    Some(match g {
        1 => &[F13],
        2 => &[F14],
        3 => &[F15],
        4 => &[F16],
        5 => &[F17],
        6 => &[F18],
        7 => &[F19],
        9 => &[F20],
        10 => &[F21],
        11 => &[F22],
        12 => &[F23],
        13 => &[F24],
        // only 12 F-keys exist
        14 => &[KANA],
        15 => &[CONVERT],
        16 => &[NONCONVERT],
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub up: bool,
}

/// A G-key that changed state, for the log
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GChange {
    pub g: u8,
    pub pressed: bool,
}

/// Tracks the G-key mask and which emulated keys are down. A key shared by several
/// G-keys (e.g. Ctrl) is pressed by the first holder and released by the last one.
#[derive(Default)]
pub struct Remapper {
    prev: u16,
    held: HashMap<u16, u32>,
}

impl Remapper {
    /// Applies a new G-key mask; key events go to `out`, changed G-keys are returned.
    pub fn update(&mut self, state: u16, out: &mut Vec<KeyEvent>) -> Vec<GChange> {
        self.update_with(state, mapping, out)
    }

    /// Releases everything still held (on exit or disconnect) so nothing gets stuck.
    pub fn release_all(&mut self, out: &mut Vec<KeyEvent>) -> Vec<GChange> {
        self.update(0, out)
    }

    pub fn update_with(
        &mut self,
        state: u16,
        map: impl Fn(u8) -> Option<&'static [Key]>,
        out: &mut Vec<KeyEvent>,
    ) -> Vec<GChange> {
        let changed = state ^ self.prev;
        let mut changes = Vec::new();
        for bit in 0..16u8 {
            if changed & (1 << bit) == 0 {
                continue;
            }
            let pressed = state & (1 << bit) != 0;
            let g = bit + 1;
            changes.push(GChange { g, pressed });
            let Some(keys) = map(g) else { continue };
            if pressed {
                for &k in keys {
                    let count = self.held.entry(k.vk).or_insert(0);
                    *count += 1;
                    if *count == 1 {
                        out.push(KeyEvent { key: k, up: false });
                    }
                }
            } else {
                for &k in keys.iter().rev() {
                    let Some(count) = self.held.get_mut(&k.vk) else { continue };
                    *count -= 1;
                    if *count == 0 {
                        self.held.remove(&k.vk);
                        out.push(KeyEvent { key: k, up: true });
                    }
                }
            }
        }
        self.prev = state;
        changes
    }
}

/// The G-key mask from a vendor report `04 xx xx b3 b4 …`, or None for any other report.
/// Bytes 1–2 change whenever bindings are edited in the A4Tech app, so they are ignored.
pub fn parse_report(data: &[u8]) -> Option<u16> {
    if data.len() < 5 || data[0] != 0x04 {
        return None;
    }
    Some(u16::from(data[3]) | (u16::from(data[4]) << 8))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(r: &mut Remapper, state: u16) -> Vec<(&'static str, bool)> {
        let mut out = Vec::new();
        r.update(state, &mut out);
        out.iter().map(|e| (e.key.name, e.up)).collect()
    }

    #[test]
    fn default_mapping_matches_the_python_version() {
        let expected = [
            (1, "F13"), (2, "F14"), (3, "F15"), (4, "F16"), (5, "F17"), (6, "F18"), (7, "F19"),
            (9, "F20"), (10, "F21"), (11, "F22"), (12, "F23"), (13, "F24"),
            (14, "Kana"), (15, "Convert"), (16, "NonConvert"),
        ];
        for (g, name) in expected {
            let keys = mapping(g).unwrap();
            assert_eq!(keys.len(), 1, "G{g}");
            assert_eq!(keys[0].name, name, "G{g}");
        }
        assert!(mapping(8).is_none(), "there is no physical G8");
    }

    #[test]
    fn press_and_release() {
        let mut r = Remapper::default();
        assert_eq!(events(&mut r, 1), [("F13", false)]);
        assert_eq!(events(&mut r, 0), [("F13", true)]);
    }

    #[test]
    fn g9_to_g16_come_from_the_second_byte() {
        let mut r = Remapper::default();
        assert_eq!(events(&mut r, 1 << 8), [("F20", false)]);
        assert_eq!(events(&mut r, 1 << 15), [("F20", true), ("NonConvert", false)]);
    }

    #[test]
    fn g8_bit_is_logged_but_sends_nothing() {
        let mut r = Remapper::default();
        let mut out = Vec::new();
        let changes = r.update(1 << 7, &mut out);
        assert_eq!(changes, [GChange { g: 8, pressed: true }]);
        assert!(out.is_empty());
    }

    #[test]
    fn several_keys_at_once_and_release_all() {
        let mut r = Remapper::default();
        assert_eq!(events(&mut r, 0b101), [("F13", false), ("F15", false)]);
        let mut out = Vec::new();
        r.release_all(&mut out);
        let names: Vec<_> = out.iter().map(|e| (e.key.name, e.up)).collect();
        assert_eq!(names, [("F13", true), ("F15", true)]);
        assert!(events(&mut r, 0).is_empty(), "nothing left to release");
    }

    #[test]
    fn shared_modifier_is_held_until_the_last_g_key_is_released() {
        fn with_ctrl(g: u8) -> Option<&'static [Key]> {
            match g {
                1 => Some(&[CTRL, F13]),
                2 => Some(&[CTRL, F14]),
                _ => None,
            }
        }
        let mut r = Remapper::default();
        let mut out = Vec::new();
        r.update_with(0b01, with_ctrl, &mut out); // G1 down
        r.update_with(0b11, with_ctrl, &mut out); // G2 down
        r.update_with(0b10, with_ctrl, &mut out); // G1 up
        r.update_with(0b00, with_ctrl, &mut out); // G2 up
        let seq: Vec<_> = out.iter().map(|e| (e.key.name, e.up)).collect();
        assert_eq!(
            seq,
            [("Ctrl", false), ("F13", false), ("F14", false), ("F13", true), ("F14", true), ("Ctrl", true)]
        );
    }

    #[test]
    fn reports() {
        assert_eq!(parse_report(&[0x04, 0x29, 0x23, 0x01, 0x00, 0, 0, 0, 0x80]), Some(1));
        assert_eq!(parse_report(&[0x04, 0xAF, 0x1F, 0x00, 0x80, 0, 0, 0, 0x80]), Some(1 << 15));
        assert_eq!(parse_report(&[0x03, 0x00, 0x00]), None);
        assert_eq!(parse_report(&[0x04, 0x00]), None);
    }
}
