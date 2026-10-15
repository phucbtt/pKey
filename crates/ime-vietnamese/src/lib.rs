//! Vietnamese syllable model: parsing, validity and tone placement (spec §8).
//!
//! The model works on [`Letter`]s (base letter + shape mark + case) plus a separate [`Tone`]
//! that belongs to the whole syllable. The displayed text is always recomputed from them.
#![forbid(unsafe_code)]

mod rhymes;

use ime_unicode::compose_char;
pub use ime_unicode::{Shape, Tone};

/// Where the tone mark goes for `oa`, `oe`, `uy` without a final consonant:
/// `Modern` → `hoà, thuý, huỷ`; `Traditional` → `hòa, thúy, hủy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToneStyle {
    #[default]
    Modern,
    Traditional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Letter {
    /// Lower-case base character (`a..z`), or a literal such as a digit.
    pub ch: char,
    pub shape: Shape,
    pub upper: bool,
    /// Key that applied `shape`; used to undo it when the same key is pressed again.
    pub mark_key: Option<char>,
    /// Telex standalone `w` that was turned into `ư`; undoing it yields a literal `w`.
    pub standalone: bool,
}

impl Letter {
    pub const fn plain(ch: char, upper: bool) -> Self {
        Letter {
            ch,
            shape: Shape::None,
            upper,
            mark_key: None,
            standalone: false,
        }
    }

    pub const fn is_vowel(&self) -> bool {
        matches!(self.ch, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
    }

    /// Lower-case display character without tone (`â`, `ơ`, `đ`, ...).
    pub const fn disp(&self) -> char {
        match (self.ch, self.shape) {
            ('a', Shape::Circumflex) => 'â',
            ('a', Shape::Breve) => 'ă',
            ('e', Shape::Circumflex) => 'ê',
            ('o', Shape::Circumflex) => 'ô',
            ('o', Shape::Horn) => 'ơ',
            ('u', Shape::Horn) => 'ư',
            ('d', Shape::Stroke) => 'đ',
            (c, _) => c,
        }
    }
}

/// Build letters from already-composed text without tone marks (`"hoa"`, `"ngươi"`). For tests/CLI.
pub fn letters_from_display(s: &str) -> Vec<Letter> {
    s.chars()
        .map(|c| {
            let upper = c.is_uppercase();
            let lc = c.to_lowercase().next().unwrap_or(c);
            let (ch, shape) = match lc {
                'ă' => ('a', Shape::Breve),
                'â' => ('a', Shape::Circumflex),
                'ê' => ('e', Shape::Circumflex),
                'ô' => ('o', Shape::Circumflex),
                'ơ' => ('o', Shape::Horn),
                'ư' => ('u', Shape::Horn),
                'đ' => ('d', Shape::Stroke),
                other => (other, Shape::None),
            };
            Letter {
                shape,
                ..Letter::plain(ch, upper)
            }
        })
        .collect()
}

/// Split of a syllable: `letters[..nuc_start]` onset, `[nuc_start..nuc_end]` nucleus
/// (vowel cluster), `[nuc_end..]` coda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Analysis {
    pub nuc_start: usize,
    pub nuc_end: usize,
}

impl Analysis {
    pub const fn has_nucleus(&self) -> bool {
        self.nuc_start < self.nuc_end
    }
}

pub fn analyze(letters: &[Letter]) -> Analysis {
    let n = letters.len();
    let mut vs = 0;
    while vs < n && !letters[vs].is_vowel() {
        vs += 1;
    }
    let mut ve = vs;
    while ve < n && letters[ve].is_vowel() {
        ve += 1;
    }
    let plain = |i: usize, c: char| letters[i].ch == c && letters[i].shape == Shape::None;
    if vs == 1 && vs < ve && plain(0, 'q') && plain(vs, 'u') {
        // `qu`: the u belongs to the onset
        vs += 1;
    } else if vs == 1 && ve - vs >= 2 && plain(0, 'g') && plain(vs, 'i') {
        // `gi` + vowel: the i belongs to the onset (gia, giêng); `gì`, `gìn` keep i as nucleus
        vs += 1;
    }
    Analysis {
        nuc_start: vs,
        nuc_end: ve,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validity {
    /// Complete valid Vietnamese syllable.
    Valid,
    /// Not valid yet but can become valid by typing more (or by adding a mark).
    Prefix,
    Invalid,
}

fn strip_shape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ă' | 'â' => 'a',
            'ê' => 'e',
            'ô' | 'ơ' => 'o',
            'ư' => 'u',
            'đ' => 'd',
            c => c,
        })
        .collect()
}

const ONSETS: &[&str] = &[
    "", "b", "c", "ch", "d", "đ", "g", "gh", "gi", "h", "k", "kh", "l", "m", "n", "ng", "ngh",
    "nh", "p", "ph", "qu", "r", "s", "t", "th", "tr", "v", "x",
];
const FOREIGN_ONSETS: &[&str] = &["f", "j", "w", "z"];

fn onset_valid(onset: &str, foreign: bool) -> bool {
    ONSETS.contains(&onset) || (foreign && FOREIGN_ONSETS.contains(&onset))
}

fn onset_is_prefix(onset: &str, foreign: bool) -> bool {
    ONSETS.iter().any(|o| o.starts_with(onset))
        || (foreign && FOREIGN_ONSETS.iter().any(|o| o.starts_with(onset)))
}

/// Spelling constraints between onset and the first vowel of the nucleus.
fn onset_nucleus_compatible(onset: &str, first: char) -> bool {
    match onset {
        "k" => matches!(first, 'i' | 'y' | 'e' | 'ê'),
        "gh" | "ngh" => matches!(first, 'i' | 'e' | 'ê'),
        "c" => !matches!(first, 'i' | 'y' | 'e' | 'ê'),
        "g" => !matches!(first, 'e' | 'ê'),
        "ng" => !matches!(first, 'i' | 'e' | 'ê'),
        _ => true,
    }
}

pub fn validity(letters: &[Letter], tone: Tone, foreign_onsets: bool) -> Validity {
    if letters.is_empty() {
        return Validity::Prefix;
    }
    if letters.iter().any(|l| !l.ch.is_ascii_alphabetic()) {
        return Validity::Invalid;
    }
    let a = analyze(letters);
    let text = |r: std::ops::Range<usize>| -> String { letters[r].iter().map(Letter::disp).collect() };
    let onset = text(0..a.nuc_start);
    let mut nucleus = text(a.nuc_start..a.nuc_end);
    // `gi` swallowed the i of `iê` (giêng, giếng): validate it as the `iê` cluster again.
    if onset == "gi" && nucleus == "ê" {
        nucleus = "iê".to_string();
    }
    let coda = text(a.nuc_end..letters.len());

    if !a.has_nucleus() {
        if !coda.is_empty() {
            return Validity::Invalid;
        }
        return if onset_is_prefix(&onset, foreign_onsets) {
            Validity::Prefix
        } else {
            Validity::Invalid
        };
    }
    // Stop finals (c, ch, p, t) only take sắc or nặng.
    if matches!(coda.as_str(), "c" | "ch" | "p" | "t")
        && matches!(tone, Tone::Huyen | Tone::Hoi | Tone::Nga)
    {
        return Validity::Invalid;
    }
    if !onset_valid(&onset, foreign_onsets) {
        return Validity::Invalid;
    }
    let first = nucleus.chars().next().unwrap_or(' ');
    if !onset_nucleus_compatible(&onset, first) {
        return Validity::Invalid;
    }
    rhymes::classify(&nucleus, &coda)
}

/// Index (into `letters`) of the vowel that carries the tone mark (spec §8.3).
pub fn tone_index(letters: &[Letter], style: ToneStyle) -> Option<usize> {
    let a = analyze(letters);
    let (vs, ve) = (a.nuc_start, a.nuc_end);
    if vs >= ve {
        return None;
    }
    // A vowel with its own mark (ă â ê ô ơ ư) wins; for `ươ` the last one (ơ).
    if let Some(i) = (vs..ve).rev().find(|&i| {
        matches!(
            letters[i].shape,
            Shape::Circumflex | Shape::Breve | Shape::Horn
        )
    }) {
        return Some(i);
    }
    let len = ve - vs;
    if len == 1 {
        return Some(vs);
    }
    if ve < letters.len() {
        // final consonant: tone goes on the last vowel (hoàn, thuyết, khoảng)
        return Some(ve - 1);
    }
    if len >= 3 {
        return Some(vs + 1);
    }
    match (letters[vs].ch, letters[vs + 1].ch) {
        ('o', 'a') | ('o', 'e') | ('u', 'y') => Some(match style {
            ToneStyle::Modern => vs + 1,
            ToneStyle::Traditional => vs,
        }),
        _ => Some(vs),
    }
}

/// Render letters + tone as NFC text.
pub fn render(letters: &[Letter], tone: Tone, style: ToneStyle) -> String {
    let ti = if tone == Tone::None {
        None
    } else {
        tone_index(letters, style)
    };
    let mut s = String::with_capacity(letters.len() * 2);
    for (i, l) in letters.iter().enumerate() {
        if l.ch.is_ascii_alphabetic() {
            let t = if Some(i) == ti { tone } else { Tone::None };
            s.push_str(&compose_char(l.ch, l.upper, l.shape, t));
        } else {
            s.push(l.ch);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(display: &str, tone: Tone, style: ToneStyle) -> String {
        render(&letters_from_display(display), tone, style)
    }

    #[test]
    fn tone_placement_spec_cases() {
        use Tone::*;
        use ToneStyle::*;
        let cases = [
            ("hoa", Huyen, Modern, "hoà"),
            ("hoa", Huyen, Traditional, "hòa"),
            ("thuy", Sac, Modern, "thuý"),
            ("thuy", Sac, Traditional, "thúy"),
            ("huy", Hoi, Modern, "huỷ"),
            ("huy", Hoi, Traditional, "hủy"),
            ("khoe", Hoi, Modern, "khoẻ"),
            ("khoe", Hoi, Traditional, "khỏe"),
            ("quy", Sac, Modern, "quý"),
            ("quy", Sac, Traditional, "quý"),
            ("gia", Sac, Modern, "giá"),
            ("gi", Huyen, Modern, "gì"),
            ("gin", Huyen, Modern, "gìn"),
            ("ngươi", Huyen, Modern, "người"),
            ("khuyên", Sac, Modern, "khuyến"),
            ("thuơ", Hoi, Modern, "thuở"),
            ("hoang", Huyen, Modern, "hoàng"),
            ("tuôi", Hoi, Modern, "tuổi"),
            ("rươu", Nang, Modern, "rượu"),
            ("quôc", Sac, Modern, "quốc"),
            ("mia", Sac, Modern, "mía"),
            ("mua", Sac, Modern, "múa"),
            ("hoai", Huyen, Modern, "hoài"),
            ("khuya", Hoi, Modern, "khuỷa"),
            ("tai", Huyen, Modern, "tài"),
            ("sao", Sac, Modern, "sáo"),
            ("lưu", Nang, Modern, "lựu"),
            ("toan", Sac, Modern, "toán"),
            ("ây", Sac, Modern, "ấy"),
        ];
        for (d, t, st, want) in cases {
            assert_eq!(show(d, t, st), want, "{d} {t:?} {st:?}");
        }
    }

    #[test]
    fn uppercase_is_preserved() {
        assert_eq!(show("ĐĂNG", Tone::None, ToneStyle::Modern), "ĐĂNG");
        assert_eq!(show("Viêt", Tone::Nang, ToneStyle::Modern), "Việt");
    }

    fn v(display: &str, tone: Tone) -> Validity {
        validity(&letters_from_display(display), tone, false)
    }

    #[test]
    fn valid_syllables() {
        for w in [
            "tiêng", "đăng", "cây", "người", "tuôi", "rươu", "quôc", "gia", "gi", "gin", "hoa",
            "thuy", "khuyên", "xoăn", "quơ", "đi", "nghi", "nghiêng", "kim", "ghe", "yêu", "uông",
            "chao", "xin", "ăn", "nhiêu", "hoang", "khuya", "quy", "quân", "giư", "giêng",
        ] {
            assert_eq!(v(w, Tone::None), Validity::Valid, "{w}");
        }
    }

    #[test]
    fn invalid_syllables() {
        for w in [
            "windows", "str", "ngi", "ci", "ghu", "tapf", "book", "ưindơs", "aa", "ơơ", "qa",
            "fa", "bcd",
        ] {
            assert_eq!(v(w, Tone::None), Validity::Invalid, "{w}");
        }
        // stop finals take only sắc/nặng
        assert_eq!(v("tap", Tone::Huyen), Validity::Invalid);
        assert_eq!(v("tap", Tone::Sac), Validity::Valid);
        assert_eq!(v("hoc", Tone::Nang), Validity::Valid);
        assert_eq!(v("hoc", Tone::Hoi), Validity::Invalid);
    }

    #[test]
    fn prefixes() {
        for w in [
            "t", "tr", "n", "ng", "ngh", "q", "qu", "tie", "tieng", "ă", "đ", "ưo", "oă", "g", "gh",
            "hoan",
        ] {
            let got = v(w, Tone::None);
            assert!(
                matches!(got, Validity::Prefix | Validity::Valid),
                "{w}: {got:?}"
            );
        }
        assert_eq!(v("tie", Tone::None), Validity::Prefix);
        assert_eq!(v("ă", Tone::None), Validity::Prefix);
    }

    #[test]
    fn foreign_onsets_option() {
        let l = letters_from_display("fa");
        assert_eq!(validity(&l, Tone::None, false), Validity::Invalid);
        assert_eq!(validity(&l, Tone::None, true), Validity::Valid);
    }

    #[test]
    fn analyze_splits() {
        let a = analyze(&letters_from_display("quân"));
        assert_eq!((a.nuc_start, a.nuc_end), (2, 3));
        let a = analyze(&letters_from_display("gia"));
        assert_eq!((a.nuc_start, a.nuc_end), (2, 3));
        let a = analyze(&letters_from_display("gi"));
        assert_eq!((a.nuc_start, a.nuc_end), (1, 2));
        let a = analyze(&letters_from_display("str"));
        assert!(!a.has_nucleus());
    }
}
