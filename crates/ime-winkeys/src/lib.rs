//! Windows key events -> engine input, shared by the TSF text service and the standalone app.
//! Everything here is pure (no Win32 calls) so it can be tested on any machine; reading the
//! keyboard state is up to the caller.

use ime_core::{KeyFlags, KeyInput, LogicalKey, Modifiers};

pub const VK_BACK: u32 = 0x08;
pub const VK_TAB: u32 = 0x09;
pub const VK_RETURN: u32 = 0x0D;
pub const VK_SHIFT: u32 = 0x10;
pub const VK_CONTROL: u32 = 0x11;
pub const VK_MENU: u32 = 0x12;
pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_SPACE: u32 = 0x20;
pub const VK_PRIOR: u32 = 0x21;
pub const VK_NEXT: u32 = 0x22;
pub const VK_END: u32 = 0x23;
pub const VK_HOME: u32 = 0x24;
pub const VK_LEFT: u32 = 0x25;
pub const VK_UP: u32 = 0x26;
pub const VK_RIGHT: u32 = 0x27;
pub const VK_DOWN: u32 = 0x28;
pub const VK_DELETE: u32 = 0x2E;
pub const VK_LWIN: u32 = 0x5B;
pub const VK_RWIN: u32 = 0x5C;
/// IME is processing the key (Windows' own IME); never ours.
pub const VK_PROCESSKEY: u32 = 0xE5;
/// Synthetic Unicode character injected through `SendInput(KEYEVENTF_UNICODE)`.
pub const VK_PACKET: u32 = 0xE7;

/// The decoded `lParam` of a `WM_KEYDOWN`/`WM_KEYUP` style message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LParam {
    pub scan: u32,
    pub extended: bool,
    /// The key was already down (auto-repeat).
    pub was_down: bool,
    /// Transition bit: the key is being released.
    pub released: bool,
}

pub fn decode_lparam(lparam: isize) -> LParam {
    let v = lparam as u32;
    LParam {
        scan: (v >> 16) & 0xFF,
        extended: v & (1 << 24) != 0,
        was_down: v & (1 << 30) != 0,
        released: v & (1 << 31) != 0,
    }
}

/// USB HID usage (page 0x07) for a set-1 scan code, so hotkeys do not depend on the layout.
/// Returns 0 for keys we do not map.
pub fn hid_from_scancode(scan: u32, extended: bool) -> u32 {
    if extended {
        return match scan {
            0x1C => 0x58, // keypad Enter
            0x1D => 0xE4, // right Ctrl
            0x35 => 0x54, // keypad /
            0x38 => 0xE6, // right Alt
            0x47 => 0x4A, // Home
            0x48 => 0x52, // Up
            0x49 => 0x4B, // Page Up
            0x4B => 0x50, // Left
            0x4D => 0x4F, // Right
            0x4F => 0x4D, // End
            0x50 => 0x51, // Down
            0x51 => 0x4E, // Page Down
            0x52 => 0x49, // Insert
            0x53 => 0x4C, // Delete
            0x5B => 0xE3, // left Win
            0x5C => 0xE7, // right Win
            _ => 0,
        };
    }
    match scan {
        0x01 => 0x29,
        0x02..=0x0A => 0x1E + (scan - 0x02), // 1..9
        0x0B => 0x27,                        // 0
        0x0C => 0x2D,
        0x0D => 0x2E,
        0x0E => 0x2A,
        0x0F => 0x2B,
        0x10 => 0x14, // q
        0x11 => 0x1A, // w
        0x12 => 0x08, // e
        0x13 => 0x15, // r
        0x14 => 0x17, // t
        0x15 => 0x1C, // y
        0x16 => 0x18, // u
        0x17 => 0x0C, // i
        0x18 => 0x12, // o
        0x19 => 0x13, // p
        0x1A => 0x2F,
        0x1B => 0x30,
        0x1C => 0x28,
        0x1D => 0xE0, // left Ctrl
        0x1E => 0x04, // a
        0x1F => 0x16, // s
        0x20 => 0x07, // d
        0x21 => 0x09, // f
        0x22 => 0x0A, // g
        0x23 => 0x0B, // h
        0x24 => 0x0D, // j
        0x25 => 0x0E, // k
        0x26 => 0x0F, // l
        0x27 => 0x33,
        0x28 => 0x34,
        0x29 => 0x35,
        0x2A => 0xE1, // left Shift
        0x2B => 0x31,
        0x2C => 0x1D, // z
        0x2D => 0x1B, // x
        0x2E => 0x06, // c
        0x2F => 0x19, // v
        0x30 => 0x05, // b
        0x31 => 0x11, // n
        0x32 => 0x10, // m
        0x33 => 0x36,
        0x34 => 0x37,
        0x35 => 0x38,
        0x36 => 0xE5, // right Shift
        0x37 => 0x55, // keypad *
        0x38 => 0xE2, // left Alt
        0x39 => 0x2C,
        0x3A => 0x39,                        // Caps Lock
        0x3B..=0x44 => 0x3A + (scan - 0x3B), // F1..F10
        0x57 => 0x44,                        // F11
        0x58 => 0x45,                        // F12
        _ => 0,
    }
}

/// The engine's view of a virtual-key code. `has_text` says whether the layout produced a
/// printable character for this key press.
pub fn logical_key(vk: u32, has_text: bool) -> LogicalKey {
    match vk {
        VK_BACK => LogicalKey::Backspace,
        VK_DELETE => LogicalKey::Delete,
        VK_RETURN => LogicalKey::Enter,
        VK_TAB => LogicalKey::Tab,
        VK_ESCAPE => LogicalKey::Escape,
        VK_SPACE => LogicalKey::Space,
        VK_LEFT => LogicalKey::Left,
        VK_RIGHT => LogicalKey::Right,
        VK_UP => LogicalKey::Up,
        VK_DOWN => LogicalKey::Down,
        VK_HOME => LogicalKey::Home,
        VK_END => LogicalKey::End,
        VK_PRIOR => LogicalKey::PageUp,
        VK_NEXT => LogicalKey::PageDown,
        _ if has_text => LogicalKey::Char,
        _ => LogicalKey::Other,
    }
}

/// Keys the text service must never touch: Windows' own IME conversion and injected Unicode.
pub fn is_foreign_key(vk: u32) -> bool {
    matches!(vk, VK_PROCESSKEY | VK_PACKET)
}

/// Is `c` a character the engine can compose with (printable, not a control character)?
pub fn is_text(c: char) -> bool {
    !c.is_control()
}

/// Build the engine input for one key event.
///
/// `text` is what the keyboard layout produces for this press (from `ToUnicodeEx`), if anything.
pub fn build_input(
    context_id: u64,
    vk: u32,
    lparam: isize,
    text: Option<char>,
    modifiers: Modifiers,
) -> KeyInput {
    let lp = decode_lparam(lparam);
    let text = text.filter(|c| is_text(*c));
    let mut flags = KeyFlags::empty();
    if lp.was_down && !lp.released {
        flags |= KeyFlags::REPEAT;
    }
    if lp.released {
        flags |= KeyFlags::KEY_UP;
    }
    let logical = logical_key(vk, text.is_some());
    KeyInput {
        context_id,
        logical_key: logical,
        text: if logical == LogicalKey::Space {
            Some(' ')
        } else {
            text
        },
        physical_code: hid_from_scancode(lp.scan, lp.extended),
        modifiers,
        flags,
    }
}

/// Build the engine input from the fields of a low-level keyboard hook event
/// (`KBDLLHOOKSTRUCT`), which has no `lParam`.
#[allow(clippy::too_many_arguments)]
pub fn build_input_raw(
    context_id: u64,
    vk: u32,
    scan: u32,
    extended: bool,
    released: bool,
    repeat: bool,
    text: Option<char>,
    modifiers: Modifiers,
) -> KeyInput {
    let mut lparam: u32 = 1 | (scan << 16);
    if extended {
        lparam |= 1 << 24;
    }
    if repeat || released {
        lparam |= 1 << 30;
    }
    if released {
        lparam |= 1 << 31;
    }
    build_input(context_id, vk, lparam as i32 as isize, text, modifiers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lparam(scan: u32, extended: bool, was_down: bool, released: bool) -> isize {
        let mut v = 1u32 | (scan << 16);
        if extended {
            v |= 1 << 24;
        }
        if was_down {
            v |= 1 << 30;
        }
        if released {
            v |= 1 << 31;
        }
        v as i32 as isize
    }

    #[test]
    fn lparam_fields() {
        let lp = decode_lparam(lparam(0x1E, false, false, false));
        assert_eq!(
            lp,
            LParam {
                scan: 0x1E,
                extended: false,
                was_down: false,
                released: false
            }
        );
        let lp = decode_lparam(lparam(0x4B, true, true, true));
        assert!(lp.extended && lp.was_down && lp.released);
        assert_eq!(lp.scan, 0x4B);
    }

    #[test]
    fn scancodes_map_to_hid_usages() {
        // Letters follow the QWERTY scan code order, but HID orders them alphabetically.
        let rows = [("qwertyuiop", 0x10), ("asdfghjkl", 0x1E), ("zxcvbnm", 0x2C)];
        for (letters, first_scan) in rows {
            for (i, c) in letters.chars().enumerate() {
                let hid = hid_from_scancode(first_scan + i as u32, false);
                assert_eq!(hid, 0x04 + (c as u32 - 'a' as u32), "{c}");
            }
        }
        assert_eq!(hid_from_scancode(0x02, false), 0x1E); // 1
        assert_eq!(hid_from_scancode(0x0A, false), 0x26); // 9
        assert_eq!(hid_from_scancode(0x0B, false), 0x27); // 0
        assert_eq!(hid_from_scancode(0x39, false), 0x2C); // Space
        assert_eq!(hid_from_scancode(0x3B, false), 0x3A); // F1
        assert_eq!(hid_from_scancode(0x44, false), 0x43); // F10
        assert_eq!(hid_from_scancode(0x57, false), 0x44); // F11
                                                          // The same scan code means a different key when extended.
        assert_eq!(hid_from_scancode(0x1D, false), 0xE0);
        assert_eq!(hid_from_scancode(0x1D, true), 0xE4);
        assert_eq!(hid_from_scancode(0x38, false), 0xE2);
        assert_eq!(hid_from_scancode(0x38, true), 0xE6);
        assert_eq!(hid_from_scancode(0x2A, false), 0xE1);
        assert_eq!(hid_from_scancode(0x36, false), 0xE5);
        assert_eq!(hid_from_scancode(0x5B, true), 0xE3);
        assert_eq!(hid_from_scancode(0xFF, false), 0);
        assert_eq!(hid_from_scancode(0x77, true), 0);
    }

    #[test]
    fn hotkey_parser_codes_agree_with_scancodes() {
        // `Hotkey` names keys by HID usage; a typed Z must produce the same number.
        let z: ime_core::Hotkey = "Alt+Z".parse().unwrap();
        assert_eq!(z.key, Some(hid_from_scancode(0x2C, false)));
        let space: ime_core::Hotkey = "Ctrl+Space".parse().unwrap();
        assert_eq!(space.key, Some(hid_from_scancode(0x39, false)));
        let f5: ime_core::Hotkey = "Ctrl+F5".parse().unwrap();
        assert_eq!(f5.key, Some(hid_from_scancode(0x3F, false)));
    }

    #[test]
    fn logical_keys() {
        assert_eq!(logical_key(VK_BACK, false), LogicalKey::Backspace);
        assert_eq!(logical_key(VK_SPACE, true), LogicalKey::Space);
        assert_eq!(logical_key(0x41, true), LogicalKey::Char);
        assert_eq!(logical_key(0x41, false), LogicalKey::Other);
        assert_eq!(logical_key(0x70, false), LogicalKey::Other); // F1
        assert_eq!(logical_key(VK_LEFT, false), LogicalKey::Left);
    }

    #[test]
    fn build_input_for_a_letter_and_space() {
        let k = build_input(
            7,
            0x41,
            lparam(0x1E, false, false, false),
            Some('a'),
            Modifiers::empty(),
        );
        assert_eq!(k.context_id, 7);
        assert_eq!(k.logical_key, LogicalKey::Char);
        assert_eq!(k.text, Some('a'));
        assert_eq!(k.physical_code, 0x04);
        assert!(k.flags.is_empty());

        let k = build_input(
            7,
            VK_SPACE,
            lparam(0x39, false, false, false),
            Some(' '),
            Modifiers::empty(),
        );
        assert_eq!(k.logical_key, LogicalKey::Space);
        assert_eq!(k.text, Some(' '));
    }

    #[test]
    fn build_input_flags_and_control_characters() {
        let k = build_input(
            1,
            0x41,
            lparam(0x1E, false, true, false),
            Some('a'),
            Modifiers::empty(),
        );
        assert!(k.flags.contains(KeyFlags::REPEAT));
        let k = build_input(
            1,
            0x41,
            lparam(0x1E, false, true, true),
            Some('a'),
            Modifiers::empty(),
        );
        assert!(k.flags.contains(KeyFlags::KEY_UP) && !k.flags.contains(KeyFlags::REPEAT));
        // Ctrl+C makes the layout return U+0003: not text.
        let k = build_input(
            1,
            0x43,
            lparam(0x2E, false, false, false),
            Some('\u{3}'),
            Modifiers::CTRL,
        );
        assert_eq!(k.text, None);
        assert_eq!(k.logical_key, LogicalKey::Other);
        assert!(k.modifiers.contains(Modifiers::CTRL));
    }

    #[test]
    fn foreign_keys() {
        assert!(is_foreign_key(VK_PROCESSKEY));
        assert!(is_foreign_key(VK_PACKET));
        assert!(!is_foreign_key(0x41));
    }

    #[test]
    fn raw_builder_matches_the_lparam_builder() {
        let a = build_input_raw(
            3,
            0x41,
            0x1E,
            false,
            false,
            true,
            Some('a'),
            Modifiers::SHIFT,
        );
        let b = build_input(
            3,
            0x41,
            lparam(0x1E, false, true, false),
            Some('a'),
            Modifiers::SHIFT,
        );
        assert_eq!(a, b);
        let up = build_input_raw(3, 0xA2, 0x1D, false, true, false, None, Modifiers::empty());
        assert!(up.flags.contains(KeyFlags::KEY_UP));
        assert_eq!(up.physical_code, 0xE0);
    }
}
