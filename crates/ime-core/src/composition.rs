//! The word currently being composed in one input context (spec §7.3).
//!
//! The displayed text is always recomputed from `letters` + `tone`; the raw keys are kept
//! separately so Esc / auto-restore can give back exactly what the user typed.

use std::fmt;

use ime_unicode::to_nfd;
use ime_vietnamese::{analyze, validity, Letter, Shape, Tone, Validity};

use crate::config::{BackspaceMode, EngineConfig};
use crate::types::{OutputEncoding, Scheme};

#[derive(Clone, Default)]
pub(crate) struct Composition {
    letters: Vec<Letter>,
    tone: Tone,
    raw: Vec<char>,
    /// A repeated mark key undid a mark: from now on the word is typed literally.
    literal: bool,
    /// The user used Backspace: raw keys no longer map one-to-one to the display.
    dirty: bool,
}

impl fmt::Debug for Composition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Composition(<redacted len={}>)", self.letters.len())
    }
}

/// Does this character belong to a word (so it starts/extends a composition)?
pub(crate) fn accepts(ch: char, cfg: &EngineConfig, composing: bool) -> bool {
    if ch.is_ascii_alphabetic() {
        return true;
    }
    match cfg.scheme {
        Scheme::Telex => cfg.telex_brackets && matches!(ch, '[' | ']'),
        Scheme::Vni => composing && ch.is_ascii_digit(),
        Scheme::Viqr => false,
    }
}

impl Composition {
    pub fn is_empty(&self) -> bool {
        self.letters.is_empty()
    }

    pub fn len(&self) -> usize {
        self.letters.len()
    }

    pub fn clear(&mut self) {
        *self = Composition::default();
    }

    pub fn raw_string(&self) -> String {
        self.raw.iter().collect()
    }

    fn display_nfc(&self, cfg: &EngineConfig) -> String {
        ime_vietnamese::render(&self.letters, self.tone, cfg.tone_style)
    }

    /// Text shown in the preedit, in the configured output form.
    pub fn render(&self, cfg: &EngineConfig) -> String {
        let s = self.display_nfc(cfg);
        match cfg.encoding {
            OutputEncoding::UnicodeNfc => s,
            OutputEncoding::UnicodeNfd => to_nfd(&s),
        }
    }

    /// Text to commit. Restores the raw keys when a mark made the word invalid Vietnamese
    /// (`windows` must not become `ưindơs`).
    pub fn finish(&self, cfg: &EngineConfig) -> String {
        if cfg.spell_check && cfg.auto_restore_invalid && !self.literal && !self.dirty {
            let raw = self.raw_string();
            let shown = self.display_nfc(cfg);
            if shown != raw
                && validity(&self.letters, self.tone, cfg.allow_foreign_onsets) != Validity::Valid
            {
                return raw;
            }
        }
        self.render(cfg)
    }

    fn acceptable(&self, cfg: &EngineConfig) -> bool {
        !cfg.spell_check
            || validity(&self.letters, self.tone, cfg.allow_foreign_onsets) != Validity::Invalid
    }

    pub fn push_key(&mut self, ch: char, cfg: &EngineConfig) {
        self.raw.push(ch);
        let upper = ch.is_ascii_uppercase();
        let key = ch.to_ascii_lowercase();
        if !self.literal && self.apply(key, upper, cfg) {
            return;
        }
        self.letters.push(Letter::plain(key, upper));
    }

    /// Returns true when `key` was consumed as a mark (or pushed its own letter).
    fn apply(&mut self, key: char, upper: bool, cfg: &EngineConfig) -> bool {
        match cfg.scheme {
            Scheme::Telex => self.apply_telex(key, upper, cfg),
            Scheme::Vni => self.apply_vni(key, upper, cfg),
            Scheme::Viqr => false,
        }
    }

    fn apply_telex(&mut self, key: char, upper: bool, cfg: &EngineConfig) -> bool {
        match key {
            's' => self.try_tone(Tone::Sac, key, upper, cfg),
            'f' => self.try_tone(Tone::Huyen, key, upper, cfg),
            'r' => self.try_tone(Tone::Hoi, key, upper, cfg),
            'x' => self.try_tone(Tone::Nga, key, upper, cfg),
            'j' => self.try_tone(Tone::Nang, key, upper, cfg),
            'z' => self.remove_marks(),
            'a' | 'e' | 'o' => self.try_shape(&[key], Shape::Circumflex, key, upper, cfg),
            'd' => self.try_stroke(key, upper, cfg),
            'w' => {
                self.try_horn_smart(
                    key,
                    upper,
                    &[('a', Shape::Breve), ('o', Shape::Horn), ('u', Shape::Horn)],
                    cfg,
                ) || self.try_standalone_w(upper, cfg)
            }
            '[' if cfg.telex_brackets => self.push_marked('o', Shape::Horn, key, upper),
            ']' if cfg.telex_brackets => self.push_marked('u', Shape::Horn, key, upper),
            _ => false,
        }
    }

    fn apply_vni(&mut self, key: char, upper: bool, cfg: &EngineConfig) -> bool {
        match key {
            '1' => self.try_tone(Tone::Sac, key, upper, cfg),
            '2' => self.try_tone(Tone::Huyen, key, upper, cfg),
            '3' => self.try_tone(Tone::Hoi, key, upper, cfg),
            '4' => self.try_tone(Tone::Nga, key, upper, cfg),
            '5' => self.try_tone(Tone::Nang, key, upper, cfg),
            '0' => self.remove_marks(),
            '6' => self.try_shape(&['a', 'e', 'o'], Shape::Circumflex, key, upper, cfg),
            '7' => self.try_horn_smart(key, upper, &[('o', Shape::Horn), ('u', Shape::Horn)], cfg),
            '8' => self.try_shape(&['a'], Shape::Breve, key, upper, cfg),
            '9' => self.try_stroke(key, upper, cfg),
            _ => false,
        }
    }

    fn push_marked(&mut self, ch: char, shape: Shape, key: char, upper: bool) -> bool {
        self.letters.push(Letter {
            shape,
            mark_key: Some(key),
            ..Letter::plain(ch, upper)
        });
        true
    }

    /// Mark the last undoable thing as undone: the key is typed literally from now on.
    fn undo_with_literal(&mut self, key: char, upper: bool) -> bool {
        self.literal = true;
        self.letters.push(Letter::plain(key, upper));
        true
    }

    fn try_tone(&mut self, tone: Tone, key: char, upper: bool, cfg: &EngineConfig) -> bool {
        if !analyze(&self.letters).has_nucleus() {
            return false;
        }
        if self.tone == tone {
            self.tone = Tone::None;
            return self.undo_with_literal(key, upper);
        }
        let old = self.tone;
        self.tone = tone;
        if self.acceptable(cfg) {
            true
        } else {
            self.tone = old;
            false
        }
    }

    /// Circumflex / breve on one of `targets` inside the vowel cluster.
    fn try_shape(
        &mut self,
        targets: &[char],
        shape: Shape,
        key: char,
        upper: bool,
        cfg: &EngineConfig,
    ) -> bool {
        let a = analyze(&self.letters);
        if !a.has_nucleus() {
            return false;
        }
        let last = self.letters.len() - 1;
        let mut seen_target = false;
        for i in (a.nuc_start..a.nuc_end).rev() {
            if !cfg.free_marking && i != last {
                break;
            }
            let l = self.letters[i];
            if !targets.contains(&l.ch) {
                continue;
            }
            let is_first_target = !seen_target;
            seen_target = true;
            if l.shape == shape {
                if is_first_target {
                    self.letters[i].shape = Shape::None;
                    self.letters[i].mark_key = None;
                    return self.undo_with_literal(key, upper);
                }
                continue;
            }
            if l.shape == Shape::None {
                self.letters[i].shape = shape;
                self.letters[i].mark_key = Some(key);
                if self.acceptable(cfg) {
                    return true;
                }
                self.letters[i].shape = Shape::None;
                self.letters[i].mark_key = None;
            }
        }
        false
    }

    /// `đ`: only the first letter can carry the stroke.
    fn try_stroke(&mut self, key: char, upper: bool, cfg: &EngineConfig) -> bool {
        let n = self.letters.len();
        if n == 0 || self.letters[0].ch != 'd' {
            return false;
        }
        if !cfg.free_marking && n != 1 {
            return false;
        }
        if self.letters[0].shape == Shape::Stroke {
            if n != 1 {
                return false;
            }
            self.letters[0].shape = Shape::None;
            self.letters[0].mark_key = None;
            return self.undo_with_literal(key, upper);
        }
        self.letters[0].shape = Shape::Stroke;
        self.letters[0].mark_key = Some(key);
        if self.acceptable(cfg) {
            true
        } else {
            self.letters[0].shape = Shape::None;
            self.letters[0].mark_key = None;
            false
        }
    }

    /// Telex `w` / VNI `7`: horn or breve on the most suitable vowel; `uo` becomes `ươ`.
    fn try_horn_smart(
        &mut self,
        key: char,
        upper: bool,
        allowed: &[(char, Shape)],
        cfg: &EngineConfig,
    ) -> bool {
        let a = analyze(&self.letters);
        if !a.has_nucleus() {
            return false;
        }
        let range = a.nuc_start..a.nuc_end;

        // Pressing the key again undoes its marks.
        if let Some(i) = range.clone().rev().find(|&i| {
            self.letters[i].mark_key == Some(key)
                && matches!(self.letters[i].shape, Shape::Horn | Shape::Breve)
        }) {
            if self.letters[i].standalone {
                let up = self.letters[i].upper;
                self.letters[i] = Letter::plain(key, up);
                self.literal = true;
                return true;
            }
            for j in range {
                if self.letters[j].mark_key == Some(key) {
                    self.letters[j].shape = Shape::None;
                    self.letters[j].mark_key = None;
                }
            }
            return self.undo_with_literal(key, upper);
        }

        // `uo` -> `ươ`
        if allowed.contains(&('u', Shape::Horn)) && allowed.contains(&('o', Shape::Horn)) {
            for i in range.start..range.end.saturating_sub(1) {
                let (u, o) = (self.letters[i], self.letters[i + 1]);
                let u_ok = u.ch == 'u' && matches!(u.shape, Shape::None | Shape::Horn);
                let o_ok = o.ch == 'o' && matches!(o.shape, Shape::None | Shape::Horn);
                if !(u_ok && o_ok) || (u.shape == Shape::Horn && o.shape == Shape::Horn) {
                    continue;
                }
                for j in [i, i + 1] {
                    if self.letters[j].shape == Shape::None {
                        self.letters[j].shape = Shape::Horn;
                        self.letters[j].mark_key = Some(key);
                    }
                }
                if self.acceptable(cfg) {
                    return true;
                }
                self.letters[i] = u;
                self.letters[i + 1] = o;
            }
        }

        // single vowel, last to first
        for i in range.rev() {
            let l = self.letters[i];
            if l.shape != Shape::None {
                continue;
            }
            if let Some(&(_, shape)) = allowed.iter().find(|(c, _)| *c == l.ch) {
                self.letters[i].shape = shape;
                self.letters[i].mark_key = Some(key);
                if self.acceptable(cfg) {
                    return true;
                }
                self.letters[i] = l;
            }
        }
        false
    }

    /// Telex `w` with no vowel in the word: `ư`.
    fn try_standalone_w(&mut self, upper: bool, cfg: &EngineConfig) -> bool {
        if !cfg.telex_w_standalone || self.letters.iter().any(Letter::is_vowel) {
            return false;
        }
        self.letters.push(Letter {
            shape: Shape::Horn,
            mark_key: Some('w'),
            standalone: true,
            ..Letter::plain('u', upper)
        });
        if self.acceptable(cfg) {
            true
        } else {
            self.letters.pop();
            false
        }
    }

    /// Telex `z` / VNI `0`: drop the tone, or else the last shape mark.
    fn remove_marks(&mut self) -> bool {
        if self.tone != Tone::None {
            self.tone = Tone::None;
            return true;
        }
        if let Some(i) = self.letters.iter().rposition(|l| l.shape != Shape::None) {
            self.letters[i].shape = Shape::None;
            self.letters[i].mark_key = None;
            return true;
        }
        false
    }

    /// Backspace. Returns whether anything is still being composed.
    pub fn backspace(&mut self, cfg: &EngineConfig) -> bool {
        match cfg.backspace_mode {
            BackspaceMode::Grapheme => {
                self.letters.pop();
                self.raw.pop();
                self.dirty = true;
                if !analyze(&self.letters).has_nucleus() {
                    self.tone = Tone::None;
                }
            }
            BackspaceMode::RawKey => {
                let mut raw = std::mem::take(&mut self.raw);
                raw.pop();
                self.clear();
                for ch in raw {
                    self.push_key(ch, cfg);
                }
            }
        }
        !self.letters.is_empty()
    }
}
