//! Text macros (spec §11): `vn` -> `Việt Nam` when a word is committed.
//!
//! The matching code ([`MacroTable`]) has no dependencies, so the engine can use it directly.
//! The `json` feature adds the `macros.json` / CSV import-export in [`store`].
//!
//! Macro text is user data: nothing in this crate formats a trigger or an expansion into an error
//! message or a log line other than the issue's own `index`.
#![forbid(unsafe_code)]

use std::collections::HashMap;

#[cfg(feature = "json")]
pub mod store;

pub const MAX_TRIGGER_CHARS: usize = 32;
pub const MAX_EXPANSION_CHARS: usize = 4096;
pub const MAX_MACROS: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum Scope {
    #[default]
    Global,
    Profile(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Macro {
    pub id: String,
    pub trigger: String,
    pub expansion: String,
    pub case_sensitive: bool,
    /// The trigger must be the whole word. When false it may also end a longer word.
    pub whole_word: bool,
    pub enabled: bool,
    pub scope: Scope,
    /// Higher wins between macros with the same trigger in different scopes.
    pub priority: i32,
    /// `VN` -> `VIỆT NAM`, `Vn` -> `Việt Nam`. Ignored for case-sensitive macros.
    pub case_follow: bool,
}

impl Macro {
    pub fn new(trigger: &str, expansion: &str) -> Self {
        Macro {
            id: String::new(),
            trigger: trigger.to_string(),
            expansion: expansion.to_string(),
            case_sensitive: false,
            whole_word: true,
            enabled: true,
            scope: Scope::Global,
            priority: 0,
            case_follow: true,
        }
    }
}

/// A validation problem; `index` is the position in the input list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub index: usize,
    pub kind: IssueKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind {
    EmptyTrigger,
    TriggerTooLong,
    /// Whitespace or control characters cannot be typed inside a word.
    BadTriggerChar,
    EmptyExpansion,
    ExpansionTooLong,
    EmptyId,
    DuplicateId,
    /// Same trigger in the same scope as an earlier macro.
    DuplicateTrigger,
    TooManyMacros,
}

fn fold(s: &str) -> String {
    s.chars().flat_map(char::to_lowercase).collect()
}

/// Check a list of macros. `max_expansion` is the user's `macros.max_expansion_chars` setting,
/// capped at [`MAX_EXPANSION_CHARS`].
pub fn validate(macros: &[Macro], max_expansion: usize) -> Vec<Issue> {
    let max_expansion = max_expansion.min(MAX_EXPANSION_CHARS);
    let mut issues = Vec::new();
    if macros.len() > MAX_MACROS {
        issues.push(Issue {
            index: MAX_MACROS,
            kind: IssueKind::TooManyMacros,
        });
    }
    let mut ids: HashMap<&str, usize> = HashMap::new();
    let mut by_trigger: HashMap<(String, &Scope), Vec<usize>> = HashMap::new();
    for (i, m) in macros.iter().enumerate() {
        let mut bad = |kind| issues.push(Issue { index: i, kind });
        let n = m.trigger.chars().count();
        if n == 0 {
            bad(IssueKind::EmptyTrigger);
        } else if n > MAX_TRIGGER_CHARS {
            bad(IssueKind::TriggerTooLong);
        } else if m
            .trigger
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            bad(IssueKind::BadTriggerChar);
        }
        let e = m.expansion.chars().count();
        if e == 0 {
            bad(IssueKind::EmptyExpansion);
        } else if e > max_expansion {
            bad(IssueKind::ExpansionTooLong);
        }
        if m.id.is_empty() {
            bad(IssueKind::EmptyId);
        } else if ids.insert(m.id.as_str(), i).is_some() {
            bad(IssueKind::DuplicateId);
        }
        let earlier = by_trigger.entry((fold(&m.trigger), &m.scope)).or_default();
        let clash = earlier.iter().any(|&j| {
            let o = &macros[j];
            !m.case_sensitive || !o.case_sensitive || o.trigger == m.trigger
        });
        if clash {
            bad(IssueKind::DuplicateTrigger);
        }
        earlier.push(i);
    }
    issues.sort_by_key(|i| i.index);
    issues
}

/// Compiled, validated macros ready for lookup.
#[derive(Clone, Debug, Default)]
pub struct MacroTable {
    entries: Vec<Macro>,
    /// folded trigger -> indices of whole-word entries
    whole: HashMap<String, Vec<usize>>,
    /// indices of entries that may match the end of a word
    suffix: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CasePattern {
    Lower,
    /// First letter upper, the rest lower (or a single upper letter).
    Title,
    Upper,
}

fn case_pattern(typed: &str) -> CasePattern {
    let letters: Vec<char> = typed.chars().filter(|c| c.is_alphabetic()).collect();
    let Some(first) = letters.first() else {
        return CasePattern::Lower;
    };
    let rest_upper = letters[1..].iter().all(|c| c.is_uppercase());
    if first.is_uppercase() && rest_upper && letters.len() > 1 {
        CasePattern::Upper
    } else if first.is_uppercase() && letters[1..].iter().all(|c| !c.is_uppercase()) {
        CasePattern::Title
    } else if letters.iter().all(|c| !c.is_uppercase()) {
        CasePattern::Lower
    } else {
        // Mixed case such as `vN`: keep the macro as written.
        CasePattern::Lower
    }
}

fn apply_case(expansion: &str, pattern: CasePattern) -> String {
    match pattern {
        CasePattern::Lower => expansion.to_string(),
        CasePattern::Upper => expansion.to_uppercase(),
        CasePattern::Title => {
            let mut cs = expansion.chars();
            match cs.next() {
                Some(c) => c.to_uppercase().chain(cs).collect(),
                None => String::new(),
            }
        }
    }
}

impl MacroTable {
    /// Build a table from validated macros; disabled ones are dropped.
    pub fn new(macros: Vec<Macro>) -> Self {
        let entries: Vec<Macro> = macros.into_iter().filter(|m| m.enabled).collect();
        let mut whole: HashMap<String, Vec<usize>> = HashMap::new();
        let mut suffix = Vec::new();
        for (i, m) in entries.iter().enumerate() {
            if m.whole_word {
                whole.entry(fold(&m.trigger)).or_default().push(i);
            } else {
                suffix.push(i);
            }
        }
        MacroTable {
            entries,
            whole,
            suffix,
        }
    }

    /// Validate and build. On problems nothing is built.
    pub fn try_new(macros: Vec<Macro>, max_expansion: usize) -> Result<Self, Vec<Issue>> {
        let issues = validate(&macros, max_expansion);
        if issues.is_empty() {
            Ok(MacroTable::new(macros))
        } else {
            Err(issues)
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn in_scope(m: &Macro, profile: Option<&str>) -> bool {
        match &m.scope {
            Scope::Global => true,
            Scope::Profile(id) => profile.is_some_and(|p| p.eq_ignore_ascii_case(id)),
        }
    }

    /// Lower rank is better: profile scope before global, then higher priority, then file order.
    fn rank(&self, i: usize) -> (u8, i32, usize) {
        let m = &self.entries[i];
        let scope = u8::from(m.scope == Scope::Global);
        (scope, -m.priority, i)
    }

    fn best(&self, candidates: impl Iterator<Item = usize>) -> Option<usize> {
        candidates.min_by_key(|&i| self.rank(i))
    }

    /// The text that should replace the word just typed, if a macro matches.
    ///
    /// `raw` is what the user typed, `shown` the composed Vietnamese text. The raw form is tried
    /// first, so a trigger that happens to be valid Vietnamese after composition still works as
    /// typed. `profile` is the id of the active profile, if any.
    pub fn expand(&self, raw: &str, shown: &str, profile: Option<&str>) -> Option<String> {
        if self.entries.is_empty() || raw.is_empty() {
            return None;
        }
        for typed in [raw, shown] {
            if let Some(out) = self.expand_one(typed, profile) {
                return Some(out);
            }
        }
        // Suffix macros replace only the end of the composed text.
        self.expand_suffix(shown, profile)
    }

    fn matches_case(m: &Macro, typed: &str) -> bool {
        !m.case_sensitive || m.trigger == typed
    }

    fn render(m: &Macro, typed: &str) -> String {
        if m.case_sensitive || !m.case_follow {
            m.expansion.clone()
        } else {
            apply_case(&m.expansion, case_pattern(typed))
        }
    }

    fn expand_one(&self, typed: &str, profile: Option<&str>) -> Option<String> {
        let ids = self.whole.get(&fold(typed))?;
        let i = self.best(
            ids.iter()
                .copied()
                .filter(|&i| Self::in_scope(&self.entries[i], profile))
                .filter(|&i| Self::matches_case(&self.entries[i], typed)),
        )?;
        Some(Self::render(&self.entries[i], typed))
    }

    fn expand_suffix(&self, shown: &str, profile: Option<&str>) -> Option<String> {
        let shown_chars: Vec<char> = shown.chars().collect();
        let candidates = self.suffix.iter().copied().filter_map(|i| {
            let m = &self.entries[i];
            if !Self::in_scope(m, profile) {
                return None;
            }
            let t: Vec<char> = m.trigger.chars().collect();
            if t.len() > shown_chars.len() {
                return None;
            }
            let split = shown_chars.len() - t.len();
            let tail: String = shown_chars[split..].iter().collect();
            let same = if m.case_sensitive {
                tail == m.trigger
            } else {
                fold(&tail) == fold(&m.trigger)
            };
            same.then_some((i, split, tail))
        });
        let (i, split, tail) = candidates.min_by_key(|(i, _, _)| self.rank(*i))?;
        let prefix: String = shown_chars[..split].iter().collect();
        Some(prefix + &Self::render(&self.entries[i], &tail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(trigger: &str, expansion: &str) -> Macro {
        Macro::new(trigger, expansion)
    }

    fn with_id(mut m: Macro, id: &str) -> Macro {
        m.id = id.to_string();
        m
    }

    fn table(list: Vec<Macro>) -> MacroTable {
        MacroTable::new(list)
    }

    #[test]
    fn basic_expansion_and_case_follow() {
        let t = table(vec![m("vn", "Việt Nam")]);
        assert_eq!(t.expand("vn", "vn", None).as_deref(), Some("Việt Nam"));
        assert_eq!(t.expand("Vn", "Vn", None).as_deref(), Some("Việt Nam"));
        assert_eq!(t.expand("VN", "VN", None).as_deref(), Some("VIỆT NAM"));
        assert_eq!(t.expand("vN", "vN", None).as_deref(), Some("Việt Nam"));
        assert_eq!(t.expand("vnn", "vnn", None), None);
        assert_eq!(t.expand("", "", None), None);
    }

    #[test]
    fn title_case_only_changes_the_first_letter() {
        let t = table(vec![m("tphcm", "thành phố Hồ Chí Minh")]);
        assert_eq!(
            t.expand("Tphcm", "Tphcm", None).as_deref(),
            Some("Thành phố Hồ Chí Minh")
        );
    }

    #[test]
    fn case_follow_off_and_case_sensitive() {
        let mut a = m("vn", "Việt Nam");
        a.case_follow = false;
        let t = table(vec![a]);
        assert_eq!(t.expand("VN", "VN", None).as_deref(), Some("Việt Nam"));

        let mut b = m("API", "Application Programming Interface");
        b.case_sensitive = true;
        let t = table(vec![b]);
        assert!(t.expand("API", "API", None).is_some());
        assert_eq!(t.expand("api", "api", None), None);
    }

    #[test]
    fn raw_and_shown_forms_both_match_raw_first() {
        let t = table(vec![m("việt", "VIET-SHOWN"), m("vieejt", "VIET-RAW")]);
        assert_eq!(
            t.expand("vieejt", "việt", None).as_deref(),
            Some("VIET-RAW")
        );
        let t = table(vec![m("việt", "VIET-SHOWN")]);
        assert_eq!(
            t.expand("vieejt", "việt", None).as_deref(),
            Some("VIET-SHOWN")
        );
    }

    #[test]
    fn disabled_macros_do_not_expand() {
        let mut a = m("vn", "x");
        a.enabled = false;
        assert!(table(vec![a]).expand("vn", "vn", None).is_none());
    }

    #[test]
    fn profile_beats_global_then_priority() {
        let mut g = m("sig", "global");
        g.priority = 100;
        let mut p = m("sig", "profile");
        p.scope = Scope::Profile("abc".into());
        let t = table(vec![g.clone(), p]);
        assert_eq!(
            t.expand("sig", "sig", Some("abc")).as_deref(),
            Some("profile")
        );
        assert_eq!(
            t.expand("sig", "sig", Some("other")).as_deref(),
            Some("global")
        );
        assert_eq!(t.expand("sig", "sig", None).as_deref(), Some("global"));

        // Same scope is rejected by validation, so priority only orders across profile scopes.
        let mut lo = m("k", "low");
        lo.scope = Scope::Profile("p".into());
        let mut hi = m("k", "high");
        hi.scope = Scope::Profile("p".into());
        hi.priority = 5;
        let t = table(vec![lo, hi]);
        assert_eq!(t.expand("k", "k", Some("P")).as_deref(), Some("high"));
    }

    #[test]
    fn suffix_macros_replace_the_end_of_the_word() {
        let mut a = m("ko", "không");
        a.whole_word = false;
        let t = table(vec![a]);
        assert_eq!(t.expand("ko", "ko", None).as_deref(), Some("không"));
        assert_eq!(
            t.expand("hanko", "hanko", None).as_deref(),
            Some("hankhông")
        );
        assert_eq!(
            t.expand("hanKO", "hanKO", None).as_deref(),
            Some("hanKHÔNG")
        );
        assert_eq!(t.expand("kon", "kon", None), None);
        // A whole-word macro is not applied inside a longer word.
        let t = table(vec![m("ko", "không")]);
        assert_eq!(t.expand("hanko", "hanko", None), None);
    }

    #[test]
    fn validation_rules() {
        use IssueKind::*;
        let ok = vec![with_id(m("vn", "Việt Nam"), "1")];
        assert!(validate(&ok, 4096).is_empty());

        let long_t = "a".repeat(MAX_TRIGGER_CHARS + 1);
        let long_e = "é".repeat(11);
        let list = vec![
            with_id(m("", "x"), "a"),
            with_id(m(&long_t, "x"), "b"),
            with_id(m("a b", "x"), "c"),
            with_id(m("ok", ""), "d"),
            with_id(m("ok2", &long_e), "e"),
            m("noid", "x"),
            with_id(m("dupid", "x"), "a"),
        ];
        let kinds: Vec<_> = validate(&list, 10)
            .iter()
            .map(|i| (i.index, i.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                (0, EmptyTrigger),
                (1, TriggerTooLong),
                (2, BadTriggerChar),
                (3, EmptyExpansion),
                (4, ExpansionTooLong),
                (5, EmptyId),
                (6, DuplicateId),
            ]
        );
    }

    #[test]
    fn duplicate_triggers() {
        let a = with_id(m("vn", "1"), "a");
        let b = with_id(m("VN", "2"), "b");
        let issues = validate(&[a.clone(), b], 100);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::DuplicateTrigger);

        // Different scope is fine.
        let mut c = with_id(m("vn", "3"), "c");
        c.scope = Scope::Profile("p".into());
        assert!(validate(&[a.clone(), c], 100).is_empty());

        // Two case-sensitive macros that differ only in case do not clash.
        let mut x = with_id(m("Ab", "1"), "x");
        let mut y = with_id(m("aB", "2"), "y");
        x.case_sensitive = true;
        y.case_sensitive = true;
        assert!(validate(&[x.clone(), y], 100).is_empty());
        // ... but a case-insensitive one clashes with them.
        let z = with_id(m("ab", "3"), "z");
        assert_eq!(validate(&[x, z], 100).len(), 1);
    }

    #[test]
    fn expansion_limit_is_capped_at_the_global_maximum() {
        let e = "x".repeat(MAX_EXPANSION_CHARS + 1);
        let list = vec![with_id(m("a", &e), "1")];
        assert_eq!(validate(&list, usize::MAX).len(), 1);
    }

    #[test]
    fn too_many_macros() {
        let list: Vec<Macro> = (0..=MAX_MACROS)
            .map(|i| with_id(m(&format!("t{i}"), "x"), &i.to_string()))
            .collect();
        let issues = validate(&list, 100);
        assert!(issues.iter().any(|i| i.kind == IssueKind::TooManyMacros));
    }

    #[test]
    fn try_new_refuses_invalid_input() {
        assert!(MacroTable::try_new(vec![m("a", "b")], 100).is_err()); // no id
        let t = MacroTable::try_new(vec![with_id(m("a", "b"), "1")], 100).unwrap();
        assert_eq!(t.len(), 1);
    }
}
