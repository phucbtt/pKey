//! IBus key events (`keyval`, `keycode`, `state`) -> engine input. Pure, so it is tested on any
//! machine without an IBus daemon.

use ime_runtime::ime_core::{ContextId, KeyFlags, KeyInput, LogicalKey, Modifiers};

/// `IBUS_RELEASE_MASK`: the event is a key release.
pub const RELEASE_MASK: u32 = 1 << 30;
const SHIFT_MASK: u32 = 1 << 0;
const LOCK_MASK: u32 = 1 << 1;
const CONTROL_MASK: u32 = 1 << 2;
/// `Mod1`, which is Alt on every layout in practice.
const ALT_MASK: u32 = 1 << 3;
/// `Mod4`: Super.
const SUPER_MASK: u32 = 1 << 6;
/// `Mod5`: ISO_Level3_Shift (AltGr).
const LEVEL3_MASK: u32 = 1 << 7;

pub fn modifiers_from_state(state: u32) -> Modifiers {
    let mut m = Modifiers::empty();
    for (mask, flag) in [
        (SHIFT_MASK, Modifiers::SHIFT),
        (LOCK_MASK, Modifiers::CAPS_LOCK),
        (CONTROL_MASK, Modifiers::CTRL),
        (ALT_MASK, Modifiers::ALT),
        (SUPER_MASK, Modifiers::META),
        (LEVEL3_MASK, Modifiers::ALT_GR),
    ] {
        if state & mask != 0 {
            m |= flag;
        }
    }
    m
}

/// X11 keysyms that are not characters.
mod sym {
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const RETURN: u32 = 0xff0d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const HOME: u32 = 0xff50;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const END: u32 = 0xff57;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const KP_0: u32 = 0xffb0;
    pub const KP_9: u32 = 0xffb9;
    pub const DELETE: u32 = 0xffff;
    pub const SHIFT_L: u32 = 0xffe1;
    pub const SHIFT_R: u32 = 0xffe2;
    pub const CONTROL_L: u32 = 0xffe3;
    pub const CONTROL_R: u32 = 0xffe4;
    pub const ALT_L: u32 = 0xffe9;
    pub const ALT_R: u32 = 0xffea;
    pub const SUPER_L: u32 = 0xffeb;
    pub const SUPER_R: u32 = 0xffec;
}

/// The character a keysym stands for, if it is a printable one. Latin-1 keysyms equal their
/// code point; the rest of Unicode is `0x0100_0000 + code point`.
pub fn keyval_to_char(keyval: u32) -> Option<char> {
    let c = match keyval {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(keyval),
        sym::KP_0..=sym::KP_9 => char::from_u32(u32::from(b'0') + (keyval - sym::KP_0)),
        v if v & 0xff00_0000 == 0x0100_0000 => char::from_u32(v & 0x00ff_ffff),
        _ => None,
    }?;
    (!c.is_control()).then_some(c)
}

fn logical_key(keyval: u32, text: Option<char>) -> LogicalKey {
    match keyval {
        sym::BACKSPACE => LogicalKey::Backspace,
        sym::DELETE => LogicalKey::Delete,
        sym::RETURN | sym::KP_ENTER => LogicalKey::Enter,
        sym::TAB => LogicalKey::Tab,
        sym::ESCAPE => LogicalKey::Escape,
        0x20 => LogicalKey::Space,
        sym::LEFT => LogicalKey::Left,
        sym::RIGHT => LogicalKey::Right,
        sym::UP => LogicalKey::Up,
        sym::DOWN => LogicalKey::Down,
        sym::HOME => LogicalKey::Home,
        sym::END => LogicalKey::End,
        sym::PAGE_UP => LogicalKey::PageUp,
        sym::PAGE_DOWN => LogicalKey::PageDown,
        _ if text.is_some() => LogicalKey::Char,
        _ => LogicalKey::Other,
    }
}

/// USB HID usage of a modifier keysym (what the hotkey matcher expects), or 0.
fn hid_of_modifier(keyval: u32) -> u32 {
    match keyval {
        sym::CONTROL_L => 0xE0,
        sym::SHIFT_L => 0xE1,
        sym::ALT_L => 0xE2,
        sym::SUPER_L => 0xE3,
        sym::CONTROL_R => 0xE4,
        sym::SHIFT_R => 0xE5,
        sym::ALT_R => 0xE6,
        sym::SUPER_R => 0xE7,
        _ => 0,
    }
}

/// HID usage for a Linux evdev key code. For the main block evdev codes equal the set-1 scan
/// codes the Windows table is written for; the keys that Windows reports as "extended" are
/// spelled out here.
fn hid_from_evdev(code: u32) -> u32 {
    match code {
        1..=0x58 => ime_winkeys::hid_from_scancode(code, false),
        96 => 0x58,  // keypad Enter
        97 => 0xE4,  // right Ctrl
        98 => 0x54,  // keypad /
        100 => 0xE6, // right Alt
        102 => 0x4A, // Home
        103 => 0x52, // Up
        104 => 0x4B, // Page Up
        105 => 0x50, // Left
        106 => 0x4F, // Right
        107 => 0x4D, // End
        108 => 0x51, // Down
        109 => 0x4E, // Page Down
        110 => 0x49, // Insert
        111 => 0x4C, // Delete
        125 => 0xE3, // left Super
        126 => 0xE7, // right Super
        _ => 0,
    }
}

/// Last resort when the client sent no key code (synthetic events): derive the HID usage of a
/// letter or digit from the character, assuming a US layout.
fn hid_from_char(c: char) -> u32 {
    match c.to_ascii_lowercase() {
        l @ 'a'..='z' => 0x04 + (l as u32 - 'a' as u32),
        d @ '1'..='9' => 0x1E + (d as u32 - '1' as u32),
        '0' => 0x27,
        ' ' => 0x2C,
        _ => 0,
    }
}

pub fn build_input(context_id: ContextId, keyval: u32, keycode: u32, state: u32) -> KeyInput {
    let text = keyval_to_char(keyval);
    let mut flags = KeyFlags::empty();
    if state & RELEASE_MASK != 0 {
        flags |= KeyFlags::KEY_UP;
    }
    let logical = logical_key(keyval, text);
    let physical = [
        hid_of_modifier(keyval),
        hid_from_evdev(keycode),
        text.map_or(0, hid_from_char),
    ]
    .into_iter()
    .find(|&c| c != 0)
    .unwrap_or(0);
    KeyInput {
        context_id,
        logical_key: logical,
        text: if logical == LogicalKey::Space {
            Some(' ')
        } else {
            text
        },
        physical_code: physical,
        modifiers: modifiers_from_state(state),
        flags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_space() {
        let k = build_input(1, 'a' as u32, 30, 0);
        assert_eq!(k.logical_key, LogicalKey::Char);
        assert_eq!(k.text, Some('a'));
        assert_eq!(k.physical_code, 0x04);
        let k = build_input(1, 0x20, 57, 0);
        assert_eq!(k.logical_key, LogicalKey::Space);
        assert_eq!(k.text, Some(' '));
    }

    #[test]
    fn unicode_keysym() {
        let k = build_input(1, 0x0100_0000 + 0x1EA1, 0, 0);
        assert_eq!(k.text, Some('ạ'));
    }

    #[test]
    fn release_and_modifiers() {
        let k = build_input(
            1,
            sym::SHIFT_L,
            42,
            RELEASE_MASK | SHIFT_MASK | CONTROL_MASK,
        );
        assert!(k.flags.contains(KeyFlags::KEY_UP));
        assert_eq!(k.logical_key, LogicalKey::Other);
        assert_eq!(k.physical_code, 0xE1);
        assert_eq!(k.text, None);
        assert_eq!(k.modifiers, Modifiers::SHIFT | Modifiers::CTRL);
    }

    #[test]
    fn named_keys() {
        for (keyval, key) in [
            (sym::BACKSPACE, LogicalKey::Backspace),
            (sym::RETURN, LogicalKey::Enter),
            (sym::TAB, LogicalKey::Tab),
            (sym::ESCAPE, LogicalKey::Escape),
            (sym::LEFT, LogicalKey::Left),
            (sym::PAGE_DOWN, LogicalKey::PageDown),
            (0xffbe, LogicalKey::Other), // F1
        ] {
            assert_eq!(build_input(1, keyval, 0, 0).logical_key, key);
        }
    }

    #[test]
    fn keypad_digits_are_text() {
        assert_eq!(build_input(1, sym::KP_0 + 5, 0, 0).text, Some('5'));
    }

    #[test]
    fn hid_falls_back_to_the_character() {
        assert_eq!(
            build_input(1, 'T' as u32, 0, SHIFT_MASK).physical_code,
            0x17
        );
        assert_eq!(build_input(1, 'z' as u32, 44, 0).physical_code, 0x1D);
    }

    #[test]
    fn altgr_state() {
        assert!(modifiers_from_state(LEVEL3_MASK).contains(Modifiers::ALT_GR));
    }
}
