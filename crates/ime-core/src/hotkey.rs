//! Hotkey syntax and matching (spec §16.3, CORE-007).
//!
//! Syntax: `Mod(+Mod)*(+Key)?` with `Mod` in `Ctrl Shift Alt Meta`. Keys are matched by their
//! layout-independent `physical_code` (USB HID usage, page 0x07), never by the typed character.
//!
//! A hotkey without a key (`Ctrl+Shift`) is *modifier-only*: it fires when the last modifier is
//! released and nothing else was pressed in between, so it never collides with `Ctrl+Shift+T`.

use std::fmt;
use std::str::FromStr;

use crate::types::{KeyFlags, KeyInput, Modifiers};

const MOD_MASK: Modifiers = Modifiers::SHIFT
    .union(Modifiers::CTRL)
    .union(Modifiers::ALT)
    .union(Modifiers::META);

/// Named (non letter/digit/function) keys and their HID usage codes.
const NAMED_KEYS: &[(&str, u32)] = &[
    ("Enter", 0x28),
    ("Escape", 0x29),
    ("Backspace", 0x2A),
    ("Tab", 0x2B),
    ("Space", 0x2C),
    ("Delete", 0x4C),
    ("Home", 0x4A),
    ("PageUp", 0x4B),
    ("End", 0x4D),
    ("PageDown", 0x4E),
    ("Right", 0x4F),
    ("Left", 0x50),
    ("Down", 0x51),
    ("Up", 0x52),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyError {
    Empty,
    /// The string contains an empty token (`Ctrl++A`, trailing `+`).
    EmptyToken,
    UnknownKey(String),
    DuplicateModifier(String),
    /// More than one non-modifier key (`Ctrl+A+B`).
    MultipleKeys,
    /// A key with no modifier would swallow ordinary typing.
    MissingModifier,
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HotkeyError::Empty => f.write_str("hotkey is empty"),
            HotkeyError::EmptyToken => f.write_str("hotkey has an empty part"),
            HotkeyError::UnknownKey(k) => write!(f, "unknown key `{k}`"),
            HotkeyError::DuplicateModifier(m) => write!(f, "modifier `{m}` appears twice"),
            HotkeyError::MultipleKeys => f.write_str("hotkey has more than one key"),
            HotkeyError::MissingModifier => f.write_str("hotkey needs at least one modifier"),
        }
    }
}

impl std::error::Error for HotkeyError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    /// Subset of `SHIFT | CTRL | ALT | META`; never empty.
    pub modifiers: Modifiers,
    /// HID usage of the key, or `None` for a modifier-only hotkey.
    pub key: Option<u32>,
}

fn modifier_from_name(token: &str) -> Option<Modifiers> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Modifiers::CTRL),
        "shift" => Some(Modifiers::SHIFT),
        "alt" | "option" => Some(Modifiers::ALT),
        "meta" | "win" | "cmd" | "super" => Some(Modifiers::META),
        _ => None,
    }
}

fn key_from_name(token: &str) -> Option<u32> {
    let lower = token.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    if bytes.len() == 1 {
        return match bytes[0] {
            c @ b'a'..=b'z' => Some(0x04 + u32::from(c - b'a')),
            b'1'..=b'9' => Some(0x1E + u32::from(bytes[0] - b'1')),
            b'0' => Some(0x27),
            _ => None,
        };
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        return match n {
            1..=12 => Some(0x3A + n - 1),
            13..=24 => Some(0x68 + n - 13),
            _ => None,
        };
    }
    NAMED_KEYS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(token))
        .map(|&(_, code)| code)
        .or(match lower.as_str() {
            "esc" => Some(0x29),
            "return" => Some(0x28),
            _ => None,
        })
}

fn key_name(code: u32) -> String {
    match code {
        0x04..=0x1D => char::from(b'A' + (code - 0x04) as u8).to_string(),
        0x1E..=0x26 => char::from(b'1' + (code - 0x1E) as u8).to_string(),
        0x27 => "0".to_string(),
        0x3A..=0x45 => format!("F{}", code - 0x3A + 1),
        0x68..=0x73 => format!("F{}", code - 0x68 + 13),
        _ => NAMED_KEYS
            .iter()
            .find(|&&(_, c)| c == code)
            .map_or_else(|| format!("0x{code:02X}"), |&(n, _)| n.to_string()),
    }
}

impl FromStr for Hotkey {
    type Err = HotkeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Err(HotkeyError::Empty);
        }
        let mut modifiers = Modifiers::empty();
        let mut key = None;
        for raw in s.split('+') {
            let token = raw.trim();
            if token.is_empty() {
                return Err(HotkeyError::EmptyToken);
            }
            if key.is_some() {
                // Anything after the key is either a second key or a modifier in the wrong place.
                return Err(HotkeyError::MultipleKeys);
            }
            if let Some(m) = modifier_from_name(token) {
                if modifiers.contains(m) {
                    return Err(HotkeyError::DuplicateModifier(token.to_string()));
                }
                modifiers |= m;
            } else if let Some(code) = key_from_name(token) {
                key = Some(code);
            } else {
                return Err(HotkeyError::UnknownKey(token.to_string()));
            }
        }
        if modifiers.is_empty() {
            return Err(HotkeyError::MissingModifier);
        }
        Ok(Hotkey { modifiers, key })
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        let mut part = |f: &mut fmt::Formatter<'_>, s: &str| {
            if !first {
                f.write_str("+")?;
            }
            first = false;
            f.write_str(s)
        };
        for (flag, name) in [
            (Modifiers::CTRL, "Ctrl"),
            (Modifiers::SHIFT, "Shift"),
            (Modifiers::ALT, "Alt"),
            (Modifiers::META, "Meta"),
        ] {
            if self.modifiers.contains(flag) {
                part(f, name)?;
            }
        }
        if let Some(code) = self.key {
            part(f, &key_name(code))?;
        }
        Ok(())
    }
}

/// Modifier a HID usage code stands for (left and right variants), if it is a modifier key.
fn modifier_of_code(code: u32) -> Option<Modifiers> {
    match code {
        0xE0 | 0xE4 => Some(Modifiers::CTRL),
        0xE1 | 0xE5 => Some(Modifiers::SHIFT),
        // Right Alt is AltGr on many layouts; treating it as Alt is the conservative choice.
        0xE2 | 0xE6 => Some(Modifiers::ALT),
        0xE3 | 0xE7 => Some(Modifiers::META),
        _ => None,
    }
}

/// Feeds key events (including modifier presses and releases) and reports when the hotkey fires.
///
/// The matcher keeps its own view of held modifiers, derived from the key codes, because
/// `KeyInput::modifiers` is a snapshot that cannot tell "Ctrl released last" from "Shift released
/// last".
#[derive(Clone, Debug)]
pub struct HotkeyMatcher {
    hotkey: Hotkey,
    held: Modifiers,
    /// Union of modifiers held since every modifier was last up.
    peak: Modifiers,
    /// A non-modifier key was pressed during the current modifier chord.
    tainted: bool,
}

impl HotkeyMatcher {
    pub fn new(hotkey: Hotkey) -> Self {
        HotkeyMatcher {
            hotkey,
            held: Modifiers::empty(),
            peak: Modifiers::empty(),
            tainted: false,
        }
    }

    pub fn hotkey(&self) -> Hotkey {
        self.hotkey
    }

    /// Forget all held state, e.g. on focus change when key-up events may have been missed.
    pub fn reset(&mut self) {
        self.held = Modifiers::empty();
        self.peak = Modifiers::empty();
        self.tainted = false;
    }

    /// Would `input` complete a `Mod+Key` hotkey right now? Does not change any state, so it can
    /// answer a "will you handle this key?" query. Always false for modifier-only hotkeys.
    pub fn would_fire(&self, input: &KeyInput) -> bool {
        let Some(key) = self.hotkey.key else {
            return false;
        };
        !input.flags.intersects(KeyFlags::KEY_UP | KeyFlags::REPEAT)
            && input.physical_code == key
            && self.held & MOD_MASK == self.hotkey.modifiers
    }

    /// Returns true when this event completes the hotkey. For `Mod+Key` hotkeys the triggering
    /// key press should then be swallowed by the adapter; modifier-only hotkeys fire on release
    /// of a modifier, which is never swallowed.
    pub fn feed(&mut self, input: &KeyInput) -> bool {
        let down = !input.flags.contains(KeyFlags::KEY_UP);
        let code = input.physical_code;
        if let Some(m) = modifier_of_code(code) {
            return self.feed_modifier(m, down);
        }
        if !down {
            return false;
        }
        self.tainted = true;
        match self.hotkey.key {
            Some(key) => {
                key == code
                    && !input.flags.contains(KeyFlags::REPEAT)
                    && self.held & MOD_MASK == self.hotkey.modifiers
            }
            None => false,
        }
    }

    fn feed_modifier(&mut self, m: Modifiers, down: bool) -> bool {
        if down {
            if self.held.is_empty() {
                self.peak = Modifiers::empty();
                self.tainted = false;
            }
            self.held |= m;
            self.peak |= m;
            return false;
        }
        self.held.remove(m);
        // Held modifiers are tracked per kind, so releasing the left Ctrl while the right Ctrl is
        // still down would clear it early. Acceptable: the chord then simply ends sooner.
        if !self.held.is_empty() {
            return false;
        }
        let fire = self.hotkey.key.is_none() && !self.tainted && self.peak == self.hotkey.modifiers;
        self.peak = Modifiers::empty();
        self.tainted = false;
        fire
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: u32 = 0xE0;
    const SHIFT: u32 = 0xE1;
    const ALT: u32 = 0xE2;
    const KEY_T: u32 = 0x17;
    const SPACE: u32 = 0x2C;

    fn ev(code: u32, down: bool) -> KeyInput {
        let mut k = KeyInput::new(1, crate::LogicalKey::Other);
        k.physical_code = code;
        if !down {
            k.flags |= KeyFlags::KEY_UP;
        }
        k
    }

    fn run(m: &mut HotkeyMatcher, events: &[(u32, bool)]) -> Vec<bool> {
        events.iter().map(|&(c, d)| m.feed(&ev(c, d))).collect()
    }

    fn parse(s: &str) -> Hotkey {
        s.parse().unwrap()
    }

    #[test]
    fn parses_and_prints_canonical_form() {
        for (src, canon) in [
            ("Ctrl+Shift", "Ctrl+Shift"),
            ("shift+ctrl", "Ctrl+Shift"),
            (" ctrl + alt + space ", "Ctrl+Alt+Space"),
            ("Cmd+Option+F12", "Alt+Meta+F12"),
            ("Ctrl+Shift+z", "Ctrl+Shift+Z"),
            ("Alt+F13", "Alt+F13"),
            ("Ctrl+0", "Ctrl+0"),
            ("Win+Esc", "Meta+Escape"),
        ] {
            let h = parse(src);
            assert_eq!(h.to_string(), canon, "{src}");
            assert_eq!(parse(&h.to_string()), h, "{src} round trip");
        }
        assert_eq!(parse("Ctrl+Space").key, Some(SPACE));
        assert_eq!(parse("Ctrl+Shift").key, None);
    }

    #[test]
    fn rejects_malformed_hotkeys() {
        use HotkeyError::*;
        assert_eq!("".parse::<Hotkey>(), Err(Empty));
        assert_eq!("  ".parse::<Hotkey>(), Err(Empty));
        assert_eq!("Ctrl++A".parse::<Hotkey>(), Err(EmptyToken));
        assert_eq!("Ctrl+".parse::<Hotkey>(), Err(EmptyToken));
        assert_eq!("Ctrl+Foo".parse::<Hotkey>(), Err(UnknownKey("Foo".into())));
        assert_eq!("Ctrl+F25".parse::<Hotkey>(), Err(UnknownKey("F25".into())));
        assert_eq!(
            "Ctrl+ctrl".parse::<Hotkey>(),
            Err(DuplicateModifier("ctrl".into()))
        );
        assert_eq!("Ctrl+A+B".parse::<Hotkey>(), Err(MultipleKeys));
        assert_eq!("Ctrl+A+Shift".parse::<Hotkey>(), Err(MultipleKeys));
        assert_eq!("A".parse::<Hotkey>(), Err(MissingModifier));
        assert_eq!("F5".parse::<Hotkey>(), Err(MissingModifier));
    }

    #[test]
    fn modifier_only_fires_on_last_release() {
        let mut m = HotkeyMatcher::new(parse("Ctrl+Shift"));
        let got = run(
            &mut m,
            &[(CTRL, true), (SHIFT, true), (SHIFT, false), (CTRL, false)],
        );
        assert_eq!(got, [false, false, false, true]);
        // The release order does not matter.
        let got = run(
            &mut m,
            &[(SHIFT, true), (CTRL, true), (SHIFT, false), (CTRL, false)],
        );
        assert_eq!(got, [false, false, false, true]);
    }

    #[test]
    fn modifier_only_is_cancelled_by_any_other_key() {
        let mut m = HotkeyMatcher::new(parse("Ctrl+Shift"));
        // Ctrl+Shift+T must not toggle.
        let got = run(
            &mut m,
            &[
                (CTRL, true),
                (SHIFT, true),
                (KEY_T, true),
                (KEY_T, false),
                (SHIFT, false),
                (CTRL, false),
            ],
        );
        assert!(got.iter().all(|&f| !f), "{got:?}");
        // A later clean chord still works.
        let got = run(
            &mut m,
            &[(CTRL, true), (SHIFT, true), (CTRL, false), (SHIFT, false)],
        );
        assert_eq!(got.last(), Some(&true));
    }

    #[test]
    fn modifier_only_needs_the_exact_set() {
        let mut m = HotkeyMatcher::new(parse("Ctrl+Shift"));
        // Only Ctrl.
        assert_eq!(run(&mut m, &[(CTRL, true), (CTRL, false)]), [false, false]);
        // Ctrl+Shift+Alt is a different chord.
        let got = run(
            &mut m,
            &[
                (CTRL, true),
                (SHIFT, true),
                (ALT, true),
                (ALT, false),
                (SHIFT, false),
                (CTRL, false),
            ],
        );
        assert!(got.iter().all(|&f| !f), "{got:?}");
    }

    #[test]
    fn modifier_plus_key_fires_on_key_down_with_exact_modifiers() {
        let mut m = HotkeyMatcher::new(parse("Ctrl+Space"));
        let got = run(&mut m, &[(CTRL, true), (SPACE, true), (SPACE, false)]);
        assert_eq!(got, [false, true, false]);
        // Extra modifier: no match.
        let got = run(&mut m, &[(SHIFT, true), (SPACE, true), (SPACE, false)]);
        assert_eq!(got, [false, false, false]);
        run(&mut m, &[(SHIFT, false), (CTRL, false)]);
        // No modifier: no match.
        assert_eq!(run(&mut m, &[(SPACE, true)]), [false]);
    }

    #[test]
    fn key_repeat_does_not_retrigger() {
        let mut m = HotkeyMatcher::new(parse("Ctrl+Space"));
        m.feed(&ev(CTRL, true));
        assert!(m.feed(&ev(SPACE, true)));
        let mut repeat = ev(SPACE, true);
        repeat.flags |= KeyFlags::REPEAT;
        assert!(!m.feed(&repeat));
    }

    #[test]
    fn reset_drops_held_state() {
        let mut m = HotkeyMatcher::new(parse("Ctrl+Shift"));
        m.feed(&ev(CTRL, true));
        m.reset();
        // The Shift press is now the first of a new chord, so Ctrl is not part of it.
        let got = run(&mut m, &[(SHIFT, true), (SHIFT, false)]);
        assert_eq!(got, [false, false]);
    }
}
