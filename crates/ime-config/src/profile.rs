//! Per-application profiles (spec §12, CORE-012): which profile applies to the focused app, and
//! the effective settings after its overrides.

use ime_core::{EngineConfig, InputMode};

use crate::types::{Config, MatchKind, MatchPattern, Profile};

/// What the adapter knows about the focused application. Fill in what the platform offers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppIdentity {
    pub exe_name: Option<String>,
    pub process_path: Option<String>,
    pub aumid: Option<String>,
    pub bundle_id: Option<String>,
    pub program: Option<String>,
    pub ibus_client: Option<String>,
}

impl AppIdentity {
    /// A stable name for "the same application" across contexts, used to remember per-app state.
    pub fn app_key(&self) -> Option<&str> {
        [
            &self.exe_name,
            &self.bundle_id,
            &self.program,
            &self.aumid,
            &self.ibus_client,
            &self.process_path,
        ]
        .into_iter()
        .find_map(|f| f.as_deref().filter(|s| !s.is_empty()))
    }

    fn field(&self, kind: MatchKind) -> Option<&str> {
        match kind {
            MatchKind::ExeName => self.exe_name.as_deref(),
            MatchKind::ProcessPath => self.process_path.as_deref(),
            MatchKind::Aumid => self.aumid.as_deref(),
            MatchKind::BundleId => self.bundle_id.as_deref(),
            MatchKind::Program => self.program.as_deref(),
            MatchKind::IbusClient => self.ibus_client.as_deref(),
        }
    }
}

/// Settings in force for one application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effective {
    pub engine: EngineConfig,
    pub macros_enabled: bool,
    /// Game mode: the engine must stay out of the way (`engine.default_mode` is English).
    pub game_mode: bool,
    /// Id of the profile that matched, if any.
    pub profile_id: Option<String>,
}

/// `*` matches any run of characters (including none), `?` exactly one. Case-insensitive.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let t: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn matches(p: &Profile, app: &AppIdentity) -> bool {
    let Some(actual) = app.field(p.matcher.kind) else {
        return false;
    };
    let want = p.matcher.value.as_str();
    match p.matcher.pattern {
        MatchPattern::Glob => glob_match(want, actual),
        MatchPattern::Exact => match p.matcher.kind {
            // File names compare case-insensitively (Windows semantics, harmless elsewhere).
            MatchKind::ExeName | MatchKind::ProcessPath => actual.eq_ignore_ascii_case(want),
            _ => actual == want,
        },
    }
}

impl Config {
    /// The winning enabled profile for `app` (spec §12.3): an exact match beats a glob match, and
    /// among equals the earlier profile in the file wins.
    pub fn matching_profile(&self, app: &AppIdentity) -> Option<&Profile> {
        let hit = |p: &&Profile| p.enabled && matches(p, app);
        let exact = |p: &&Profile| p.matcher.pattern == MatchPattern::Exact;
        self.profiles
            .iter()
            .filter(hit)
            .find(exact)
            .or_else(|| self.profiles.iter().find(hit))
    }

    /// Global settings with the matching profile's overrides applied.
    pub fn resolve(&self, app: &AppIdentity) -> Effective {
        let mut engine = self.engine_config();
        let mut macros_enabled = self.macros.enabled;
        let mut game_mode = false;
        let profile = self.matching_profile(app);
        if let Some(p) = profile {
            let o = &p.overrides;
            if let Some(v) = o.mode {
                engine.default_mode = v;
            }
            if let Some(v) = o.scheme {
                engine.scheme = v;
            }
            if let Some(v) = o.encoding {
                engine.encoding = v;
            }
            if let Some(v) = o.tone_style {
                engine.tone_style = v;
            }
            if let Some(v) = o.spell_check {
                engine.spell_check = v;
            }
            if let Some(v) = o.macros_enabled {
                macros_enabled = v;
            }
            game_mode = o.game_mode.unwrap_or(false);
        }
        if game_mode {
            engine.default_mode = InputMode::English;
            macros_enabled = false;
        }
        Effective {
            engine,
            macros_enabled,
            game_mode,
            profile_id: profile.map(|p| p.id.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ProfileMatch, ProfileOverride};
    use ime_core::Scheme;

    fn profile(id: &str, kind: MatchKind, value: &str, pattern: MatchPattern) -> Profile {
        Profile {
            id: id.into(),
            name: id.into(),
            enabled: true,
            matcher: ProfileMatch {
                kind,
                value: value.into(),
                pattern,
            },
            overrides: ProfileOverride::default(),
        }
    }

    fn exe(name: &str) -> AppIdentity {
        AppIdentity {
            exe_name: Some(name.into()),
            ..Default::default()
        }
    }

    #[test]
    fn glob_cases() {
        for (p, t, want) in [
            ("*", "anything", true),
            ("*.exe", "Code.EXE", true),
            ("code*", "code.exe", true),
            ("c?de.exe", "code.exe", true),
            ("c?de.exe", "cde.exe", false),
            ("*game*", "my-game-2.exe", true),
            ("a*b*c", "aXXbYYc", true),
            ("a*b*c", "aXXbYY", false),
            ("", "", true),
            ("", "x", false),
            ("**", "", true),
        ] {
            assert_eq!(glob_match(p, t), want, "{p} vs {t}");
        }
    }

    #[test]
    fn exact_match_rules() {
        let mut c = Config::default();
        c.profiles.push(profile(
            "a",
            MatchKind::ExeName,
            "code.exe",
            MatchPattern::Exact,
        ));
        c.profiles.push(profile(
            "b",
            MatchKind::BundleId,
            "com.Foo.App",
            MatchPattern::Exact,
        ));
        assert!(c.matching_profile(&exe("Code.EXE")).is_some());
        assert!(c.matching_profile(&exe("code2.exe")).is_none());
        let mac = |b: &str| AppIdentity {
            bundle_id: Some(b.into()),
            ..Default::default()
        };
        assert!(c.matching_profile(&mac("com.Foo.App")).is_some());
        assert!(
            c.matching_profile(&mac("com.foo.app")).is_none(),
            "bundle ids are case-sensitive"
        );
        // The field the profile needs is not known for this app.
        assert!(c.matching_profile(&AppIdentity::default()).is_none());
    }

    #[test]
    fn first_enabled_match_wins_and_disabled_is_skipped() {
        let mut c = Config::default();
        let mut off = profile("off", MatchKind::ExeName, "*", MatchPattern::Glob);
        off.enabled = false;
        c.profiles.push(off);
        c.profiles.push(profile(
            "first",
            MatchKind::ExeName,
            "*.exe",
            MatchPattern::Glob,
        ));
        c.profiles.push(profile(
            "second",
            MatchKind::ExeName,
            "?.exe",
            MatchPattern::Glob,
        ));
        assert_eq!(
            c.resolve(&exe("a.exe")).profile_id.as_deref(),
            Some("first")
        );
    }

    #[test]
    fn exact_beats_glob_even_when_later() {
        let mut c = Config::default();
        c.profiles.push(profile(
            "glob",
            MatchKind::ExeName,
            "*.exe",
            MatchPattern::Glob,
        ));
        c.profiles.push(profile(
            "exact",
            MatchKind::ExeName,
            "a.exe",
            MatchPattern::Exact,
        ));
        assert_eq!(
            c.resolve(&exe("a.exe")).profile_id.as_deref(),
            Some("exact")
        );
        assert_eq!(c.resolve(&exe("b.exe")).profile_id.as_deref(), Some("glob"));
    }

    #[test]
    fn overrides_apply_only_to_matching_apps() {
        let mut c = Config::default();
        let mut p = profile("p", MatchKind::ExeName, "code.exe", MatchPattern::Exact);
        p.overrides = ProfileOverride {
            mode: Some(InputMode::English),
            scheme: Some(Scheme::Vni),
            spell_check: Some(false),
            macros_enabled: Some(false),
            ..Default::default()
        };
        c.profiles.push(p);

        let hit = c.resolve(&exe("code.exe"));
        assert_eq!(hit.engine.default_mode, InputMode::English);
        assert_eq!(hit.engine.scheme, Scheme::Vni);
        assert!(!hit.engine.spell_check);
        assert!(!hit.macros_enabled);
        assert!(!hit.game_mode);

        let miss = c.resolve(&exe("notepad.exe"));
        assert_eq!(miss.engine, c.engine_config());
        assert!(miss.macros_enabled && miss.profile_id.is_none());
    }

    #[test]
    fn game_mode_forces_english_and_no_macros() {
        let mut c = Config::default();
        let mut p = profile("g", MatchKind::ExeName, "game.exe", MatchPattern::Exact);
        p.overrides.game_mode = Some(true);
        p.overrides.mode = Some(InputMode::Vietnamese);
        c.profiles.push(p);
        let e = c.resolve(&exe("game.exe"));
        assert!(e.game_mode && !e.macros_enabled);
        assert_eq!(e.engine.default_mode, InputMode::English);
    }
}
