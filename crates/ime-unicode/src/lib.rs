//! Unicode helpers for VietIME: shape/tone marks, NFC/NFD rendering and caret offsets.
//!
//! No I/O and no OS API (spec §6.2, §10).
#![forbid(unsafe_code)]

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// Letter-shape mark (dấu mũ, trăng, móc, gạch ngang của đ).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Shape {
    #[default]
    None,
    Circumflex,
    Breve,
    Horn,
    Stroke,
}

/// Tone mark (dấu thanh).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    None,
    Sac,
    Huyen,
    Hoi,
    Nga,
    Nang,
}

impl Tone {
    pub const fn combining(self) -> Option<char> {
        match self {
            Tone::None => None,
            Tone::Sac => Some('\u{0301}'),
            Tone::Huyen => Some('\u{0300}'),
            Tone::Hoi => Some('\u{0309}'),
            Tone::Nga => Some('\u{0303}'),
            Tone::Nang => Some('\u{0323}'),
        }
    }
}

impl Shape {
    pub const fn combining(self) -> Option<char> {
        match self {
            Shape::Circumflex => Some('\u{0302}'),
            Shape::Breve => Some('\u{0306}'),
            Shape::Horn => Some('\u{031B}'),
            Shape::None | Shape::Stroke => None,
        }
    }
}

/// Compose one letter (NFC). `đ/Đ` is a precomposed letter and cannot carry a tone mark.
pub fn compose_char(base: char, upper: bool, shape: Shape, tone: Tone) -> String {
    if shape == Shape::Stroke && base.eq_ignore_ascii_case(&'d') {
        return if upper { "Đ" } else { "đ" }.to_string();
    }
    let base = if upper {
        base.to_uppercase().next().unwrap_or(base)
    } else {
        base
    };
    let mut s = String::with_capacity(8);
    s.push(base);
    if let Some(c) = shape.combining() {
        s.push(c);
    }
    if let Some(c) = tone.combining() {
        s.push(c);
    }
    s.nfc().collect()
}

pub fn to_nfc(s: &str) -> String {
    s.nfc().collect()
}

pub fn to_nfd(s: &str) -> String {
    s.nfd().collect()
}

pub fn is_nfc(s: &str) -> bool {
    unicode_normalization::is_nfc(s)
}

/// Caret position at the end of a text, in every unit an adapter may need (spec §5, §7.1).
/// Windows/macOS text APIs use UTF-16, IBus uses scalars, Fcitx5 uses UTF-8 bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caret {
    pub graphemes: u32,
    pub utf16: u32,
    pub utf8: u32,
    pub scalars: u32,
}

pub fn caret_at_end(text: &str) -> Caret {
    Caret {
        graphemes: text.graphemes(true).count() as u32,
        utf16: text.encode_utf16().count() as u32,
        utf8: text.len() as u32,
        scalars: text.chars().count() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_known_letters() {
        assert_eq!(compose_char('a', false, Shape::Circumflex, Tone::Sac), "ấ");
        assert_eq!(compose_char('o', false, Shape::Horn, Tone::Nang), "ợ");
        assert_eq!(compose_char('a', true, Shape::Breve, Tone::Hoi), "Ẳ");
        assert_eq!(compose_char('d', false, Shape::Stroke, Tone::None), "đ");
        assert_eq!(compose_char('d', true, Shape::Stroke, Tone::Sac), "Đ");
        assert_eq!(compose_char('y', false, Shape::None, Tone::Nga), "ỹ");
    }

    #[test]
    fn every_vietnamese_vowel_is_one_scalar_and_nfc() {
        let vowels: &[(char, &[Shape])] = &[
            ('a', &[Shape::None, Shape::Circumflex, Shape::Breve]),
            ('e', &[Shape::None, Shape::Circumflex]),
            ('i', &[Shape::None]),
            ('o', &[Shape::None, Shape::Circumflex, Shape::Horn]),
            ('u', &[Shape::None, Shape::Horn]),
            ('y', &[Shape::None]),
        ];
        let tones = [
            Tone::None,
            Tone::Sac,
            Tone::Huyen,
            Tone::Hoi,
            Tone::Nga,
            Tone::Nang,
        ];
        let mut non_ascii = 0;
        for (base, shapes) in vowels {
            for &shape in *shapes {
                for &tone in &tones {
                    for upper in [false, true] {
                        let s = compose_char(*base, upper, shape, tone);
                        assert_eq!(s.chars().count(), 1, "{base} {shape:?} {tone:?}");
                        assert!(is_nfc(&s));
                        if !s.is_ascii() {
                            non_ascii += 1;
                        }
                    }
                }
            }
        }
        // 66 non-ASCII vowel forms * 2 cases + đ/Đ = 134 (spec appendix A)
        assert_eq!(non_ascii + 2, 134);
    }

    #[test]
    fn nfd_uses_canonical_mark_order() {
        assert_eq!(to_nfd("ợ"), "o\u{031B}\u{0323}");
        assert_eq!(to_nfd("ấ"), "a\u{0302}\u{0301}");
        assert_eq!(to_nfd("đ"), "đ");
        assert_eq!(to_nfc(&to_nfd("tiếng Việt")), "tiếng Việt");
    }

    #[test]
    fn caret_units() {
        let c = caret_at_end("tiếng");
        assert_eq!(c.graphemes, 5);
        assert_eq!(c.scalars, 5);
        assert_eq!(c.utf16, 5);
        assert_eq!(c.utf8, 7);
        let c = caret_at_end(&to_nfd("tiếng"));
        assert_eq!(c.graphemes, 5);
        assert_eq!(c.scalars, 7);
        assert_eq!(c.utf16, 7);
        let c = caret_at_end("a😀");
        assert_eq!((c.graphemes, c.utf16, c.utf8, c.scalars), (2, 3, 5, 2));
    }
}
