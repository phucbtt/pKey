//! Exhaustive check of the key -> mark pipeline.
//!
//! Every valid Vietnamese syllable (onset x rhyme x final x tone, from the tables in
//! `ime-vietnamese`) is encoded into the keys a person would type in Telex, VNI and VIQR by an
//! encoder written independently of the engine, typed into a fake host, and the committed text
//! must equal the syllable. Tone *placement* is not what this test checks (that has explicit
//! cases in `ime-vietnamese`); it checks that every mark key lands and nothing is undone,
//! rejected or restored by mistake.

use ime_core::{EngineConfig, Scheme, ToneStyle};
use ime_testing::FakeHost;
use ime_vietnamese::{
    letters_from_display, onsets, render, rhymes, validity, Letter, Shape, Tone, Validity,
};

const TONES: [Tone; 6] = [
    Tone::None,
    Tone::Sac,
    Tone::Huyen,
    Tone::Hoi,
    Tone::Nga,
    Tone::Nang,
];

/// Keys for one letter *without* its tone.
fn letter_keys(l: &Letter, scheme: Scheme) -> String {
    let c = l.ch;
    let marked = match (c, l.shape, scheme) {
        (_, Shape::None, _) => None,
        ('a', Shape::Breve, Scheme::Telex) => Some("aw"),
        ('a', Shape::Circumflex, Scheme::Telex) => Some("aa"),
        ('e', Shape::Circumflex, Scheme::Telex) => Some("ee"),
        ('o', Shape::Circumflex, Scheme::Telex) => Some("oo"),
        ('o', Shape::Horn, Scheme::Telex) => Some("ow"),
        ('u', Shape::Horn, Scheme::Telex) => Some("uw"),
        ('d', Shape::Stroke, Scheme::Telex | Scheme::Viqr) => Some("dd"),
        ('a', Shape::Breve, Scheme::Vni) => Some("a8"),
        ('a', Shape::Circumflex, Scheme::Vni) => Some("a6"),
        ('e', Shape::Circumflex, Scheme::Vni) => Some("e6"),
        ('o', Shape::Circumflex, Scheme::Vni) => Some("o6"),
        ('o', Shape::Horn, Scheme::Vni) => Some("o7"),
        ('u', Shape::Horn, Scheme::Vni) => Some("u7"),
        ('d', Shape::Stroke, Scheme::Vni) => Some("d9"),
        ('a', Shape::Breve, Scheme::Viqr) => Some("a("),
        ('a', Shape::Circumflex, Scheme::Viqr) => Some("a^"),
        ('e', Shape::Circumflex, Scheme::Viqr) => Some("e^"),
        ('o', Shape::Circumflex, Scheme::Viqr) => Some("o^"),
        ('o', Shape::Horn, Scheme::Viqr) => Some("o+"),
        ('u', Shape::Horn, Scheme::Viqr) => Some("u+"),
        _ => panic!("no encoding for {c:?} {:?}", l.shape),
    };
    marked.map_or_else(|| c.to_string(), str::to_string)
}

fn tone_key(t: Tone, scheme: Scheme) -> &'static str {
    match (t, scheme) {
        (Tone::None, _) => "",
        (Tone::Sac, Scheme::Telex) => "s",
        (Tone::Huyen, Scheme::Telex) => "f",
        (Tone::Hoi, Scheme::Telex) => "r",
        (Tone::Nga, Scheme::Telex) => "x",
        (Tone::Nang, Scheme::Telex) => "j",
        (Tone::Sac, Scheme::Vni) => "1",
        (Tone::Huyen, Scheme::Vni) => "2",
        (Tone::Hoi, Scheme::Vni) => "3",
        (Tone::Nga, Scheme::Vni) => "4",
        (Tone::Nang, Scheme::Vni) => "5",
        (Tone::Sac, Scheme::Viqr) => "'",
        (Tone::Huyen, Scheme::Viqr) => "`",
        (Tone::Hoi, Scheme::Viqr) => "?",
        (Tone::Nga, Scheme::Viqr) => "~",
        (Tone::Nang, Scheme::Viqr) => ".",
    }
}

struct Syllable {
    letters: Vec<Letter>,
    tone: Tone,
}

/// All syllables the validity rules accept, including the stop-final tone restriction.
fn all_syllables() -> Vec<Syllable> {
    let mut out = Vec::new();
    for onset in onsets() {
        for (nucleus, codas) in rhymes() {
            for coda in *codas {
                let word = format!("{onset}{nucleus}{coda}");
                let letters = letters_from_display(&word);
                for tone in TONES {
                    if validity(&letters, tone, false) == Validity::Valid {
                        out.push(Syllable {
                            letters: letters.clone(),
                            tone,
                        });
                    }
                }
            }
        }
    }
    out
}

fn cfg(scheme: Scheme, style: ToneStyle) -> EngineConfig {
    EngineConfig {
        scheme,
        tone_style: style,
        ..EngineConfig::default()
    }
}

fn typed(scheme: Scheme, style: ToneStyle, keys: &str) -> String {
    let mut h = FakeHost::new(cfg(scheme, style));
    h.feed(keys).expect("keys parse");
    h.feed(" ").unwrap();
    assert!(h.violations.is_empty(), "{keys:?}: {:?}", h.violations);
    h.visible()
}

fn keys_for(s: &Syllable, scheme: Scheme) -> String {
    let mut keys: String = s.letters.iter().map(|l| letter_keys(l, scheme)).collect();
    keys.push_str(tone_key(s.tone, scheme));
    keys
}

fn check(scheme: Scheme, style: ToneStyle, stride: usize, case: fn(&str) -> String) {
    let syllables = all_syllables();
    assert!(syllables.len() > 5000, "only {} syllables", syllables.len());
    let mut failures = Vec::new();
    let mut checked = 0;
    for s in syllables.iter().step_by(stride) {
        let expected = case(&render(&s.letters, s.tone, style));
        let keys = case(&keys_for(s, scheme));
        // Upper-casing a key script upper-cases mark keys too; the engine ignores key case.
        let got = typed(scheme, style, &keys);
        checked += 1;
        if got != format!("{expected} ") {
            failures.push(format!("{keys:?} -> {got:?}, expected {expected:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{scheme:?} {style:?}: {} of {checked} syllables failed, first 25:\n{}",
        failures.len(),
        failures
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn same(s: &str) -> String {
    s.to_string()
}

fn title(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn upper(s: &str) -> String {
    s.to_uppercase()
}

#[test]
fn telex_all_syllables() {
    check(Scheme::Telex, ToneStyle::Modern, 1, same);
}

#[test]
fn vni_all_syllables() {
    check(Scheme::Vni, ToneStyle::Modern, 1, same);
}

#[test]
fn viqr_all_syllables() {
    check(Scheme::Viqr, ToneStyle::Modern, 1, same);
}

#[test]
fn traditional_tone_style() {
    check(Scheme::Telex, ToneStyle::Traditional, 1, same);
    check(Scheme::Vni, ToneStyle::Traditional, 3, same);
}

#[test]
fn capitalised_and_shouted_words() {
    // The first letter's case is the user's; the rest are lower case. For the shouted form every
    // key (mark keys included) is upper case.
    for scheme in [Scheme::Telex, Scheme::Vni, Scheme::Viqr] {
        check(scheme, ToneStyle::Modern, 7, title_both);
        check(scheme, ToneStyle::Modern, 7, upper);
    }
}

/// Title case applies to the *keys* and to the expected text alike: only the first letter
/// differs from the lower-case case.
fn title_both(s: &str) -> String {
    title(s)
}

#[test]
fn the_generator_covers_the_whole_table() {
    let all = all_syllables();
    // A few well-known syllables must be present, so a broken generator cannot pass silently.
    for (word, tone) in [
        ("tiêng", Tone::Sac),
        ("nghiêng", Tone::Nang),
        ("quân", Tone::None),
        ("giêng", Tone::Sac),
        ("đương", Tone::Huyen),
        ("ươn", Tone::Huyen),
    ] {
        let want = letters_from_display(word);
        assert!(
            all.iter().any(|s| s.letters == want && s.tone == tone),
            "{word} {tone:?}"
        );
    }
}
