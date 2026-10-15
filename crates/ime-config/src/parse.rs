//! TOML text -> [`Config`]: migration, then strict validation that reports every problem at once.

use ime_core::{BackspaceMode, Hotkey, InputMode, OutputEncoding, RestoreKey, Scheme, ToneStyle};
use toml_edit::{DocumentMut, Item, TableLike};

use crate::migrate::migrate_document;
use crate::types::*;

pub const SCHEMA_VERSION: i64 = 1;

const MAX_NAME_CHARS: usize = 100;
const MAX_MATCH_VALUE_CHARS: usize = 1024;
const MAX_HOOK_APPS: usize = 256;

pub(crate) const MODES: &[(&str, InputMode)] =
    &[("vi", InputMode::Vietnamese), ("en", InputMode::English)];
pub(crate) const SCOPES: &[(&str, ModeScope)] = &[
    ("global", ModeScope::Global),
    ("per_app", ModeScope::PerApp),
];
pub(crate) const SCHEMES: &[(&str, Scheme)] = &[
    ("telex", Scheme::Telex),
    ("vni", Scheme::Vni),
    ("viqr", Scheme::Viqr),
];
pub(crate) const ENCODINGS: &[(&str, OutputEncoding)] = &[
    ("unicode_nfc", OutputEncoding::UnicodeNfc),
    ("unicode_nfd", OutputEncoding::UnicodeNfd),
];
pub(crate) const LANGS: &[(&str, UiLanguage)] = &[
    ("system", UiLanguage::System),
    ("vi", UiLanguage::Vi),
    ("en", UiLanguage::En),
];
pub(crate) const THEMES: &[(&str, Theme)] = &[
    ("system", Theme::System),
    ("light", Theme::Light),
    ("dark", Theme::Dark),
];
pub(crate) const TONE_STYLES: &[(&str, ToneStyle)] = &[
    ("modern", ToneStyle::Modern),
    ("traditional", ToneStyle::Traditional),
];
pub(crate) const RESTORE_KEYS: &[(&str, RestoreKey)] =
    &[("esc", RestoreKey::Esc), ("none", RestoreKey::None)];
pub(crate) const BACKSPACES: &[(&str, BackspaceMode)] = &[
    ("grapheme", BackspaceMode::Grapheme),
    ("raw_key", BackspaceMode::RawKey),
];
pub(crate) const DIAGNOSTICS: &[(&str, DiagnosticsLevel)] = &[
    ("off", DiagnosticsLevel::Off),
    ("basic", DiagnosticsLevel::Basic),
    ("verbose", DiagnosticsLevel::Verbose),
];
pub(crate) const CHANNELS: &[(&str, UpdateChannel)] = &[
    ("stable", UpdateChannel::Stable),
    ("beta", UpdateChannel::Beta),
];
pub(crate) const EXPAND_ON: &[(&str, ExpandOn)] = &[
    ("space", ExpandOn::Space),
    ("enter", ExpandOn::Enter),
    ("tab", ExpandOn::Tab),
    ("punctuation", ExpandOn::Punctuation),
];
pub(crate) const MATCH_KINDS: &[(&str, MatchKind)] = &[
    ("exe_name", MatchKind::ExeName),
    ("process_path", MatchKind::ProcessPath),
    ("aumid", MatchKind::Aumid),
    ("bundle_id", MatchKind::BundleId),
    ("program", MatchKind::Program),
    ("ibus_client", MatchKind::IbusClient),
];
pub(crate) const PATTERNS: &[(&str, MatchPattern)] =
    &[("exact", MatchPattern::Exact), ("glob", MatchPattern::Glob)];

pub(crate) fn name_of<T: Copy + PartialEq>(table: &[(&'static str, T)], v: T) -> &'static str {
    table.iter().find(|(_, t)| *t == v).map_or("", |(n, _)| n)
}

/// Result of a successful load.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub config: Config,
    /// Non-fatal findings: unknown keys (kept in the file), a newer schema version, ...
    pub warnings: Vec<Issue>,
    /// The schema version found in the file when it had to be migrated.
    pub migrated_from: Option<i64>,
}

#[derive(Default)]
struct Ctx {
    errors: Vec<Issue>,
    warnings: Vec<Issue>,
}

type Tbl<'a> = Option<&'a dyn TableLike>;

impl Ctx {
    fn err(&mut self, path: &str, message: impl Into<String>) {
        self.errors.push(Issue {
            path: path.to_string(),
            message: message.into(),
        });
    }

    fn warn(&mut self, path: &str, message: impl Into<String>) {
        self.warnings.push(Issue {
            path: path.to_string(),
            message: message.into(),
        });
    }

    fn section<'a>(&mut self, parent: &'a dyn TableLike, key: &str) -> Tbl<'a> {
        match parent.get(key) {
            None => None,
            Some(item) => match item.as_table_like() {
                Some(t) => Some(t),
                None => {
                    self.err(key, "expected a table");
                    None
                }
            },
        }
    }

    fn unknown_keys(&mut self, section: &str, t: Tbl<'_>, known: &[&str]) {
        if let Some(t) = t {
            for (k, _) in t.iter() {
                if !known.contains(&k) {
                    let path = if section.is_empty() {
                        k.to_string()
                    } else {
                        format!("{section}.{k}")
                    };
                    self.warn(&path, "unknown key (kept as is)");
                }
            }
        }
    }

    fn item<'a>(&self, t: Tbl<'a>, key: &str) -> Option<&'a Item> {
        t.and_then(|t| t.get(key))
    }

    fn boolean(&mut self, t: Tbl<'_>, sect: &str, key: &str, default: bool) -> bool {
        self.opt_bool(t, sect, key).unwrap_or(default)
    }

    fn opt_bool(&mut self, t: Tbl<'_>, sect: &str, key: &str) -> Option<bool> {
        let item = self.item(t, key)?;
        match item.as_bool() {
            Some(b) => Some(b),
            None => {
                self.err(&join(sect, key), "expected true or false");
                None
            }
        }
    }

    fn opt_str(&mut self, t: Tbl<'_>, sect: &str, key: &str) -> Option<String> {
        let item = self.item(t, key)?;
        match item.as_str() {
            Some(s) => Some(s.to_string()),
            None => {
                self.err(&join(sect, key), "expected a string");
                None
            }
        }
    }

    fn opt_choice<T: Copy>(
        &mut self,
        t: Tbl<'_>,
        sect: &str,
        key: &str,
        names: &[(&str, T)],
    ) -> Option<T> {
        let s = self.opt_str(t, sect, key)?;
        match names.iter().find(|(n, _)| *n == s) {
            Some(&(_, v)) => Some(v),
            None => {
                let allowed: Vec<&str> = names.iter().map(|(n, _)| *n).collect();
                self.err(
                    &join(sect, key),
                    format!(
                        "`{s}` is not valid, expected one of: {}",
                        allowed.join(", ")
                    ),
                );
                None
            }
        }
    }

    fn choice<T: Copy>(
        &mut self,
        t: Tbl<'_>,
        sect: &str,
        key: &str,
        default: T,
        names: &[(&str, T)],
    ) -> T {
        self.opt_choice(t, sect, key, names).unwrap_or(default)
    }

    fn string_list(&mut self, t: Tbl<'_>, sect: &str, key: &str) -> Option<Vec<String>> {
        let item = self.item(t, key)?;
        let path = join(sect, key);
        let Some(arr) = item.as_array() else {
            self.err(&path, "expected a list of strings");
            return None;
        };
        let mut out = Vec::new();
        for v in arr.iter() {
            match v.as_str() {
                Some(s) => out.push(s.to_string()),
                None => self.err(&path, "every list entry must be a string"),
            }
        }
        Some(out)
    }

    fn hotkey(
        &mut self,
        t: Tbl<'_>,
        sect: &str,
        key: &str,
        default: Option<Hotkey>,
    ) -> Option<Hotkey> {
        let Some(s) = self.opt_str(t, sect, key) else {
            return default;
        };
        if s.trim().is_empty() {
            return None;
        }
        match s.parse::<Hotkey>() {
            Ok(h) => Some(h),
            Err(e) => {
                self.err(&join(sect, key), e.to_string());
                default
            }
        }
    }
}

fn join(sect: &str, key: &str) -> String {
    if sect.is_empty() {
        key.to_string()
    } else {
        format!("{sect}.{key}")
    }
}

fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

impl Config {
    /// Parse, migrate and validate a configuration file's text.
    pub fn parse(text: &str) -> Result<Loaded, ConfigError> {
        let mut doc: DocumentMut = text
            .parse()
            .map_err(|e: toml_edit::TomlError| ConfigError::Syntax(e.to_string()))?;
        let mut cx = Ctx::default();

        let found = match doc.get("schema_version") {
            None => SCHEMA_VERSION,
            Some(item) => match item.as_integer() {
                Some(v) if v >= 1 => v,
                _ => {
                    return Err(ConfigError::Invalid(vec![Issue {
                        path: "schema_version".into(),
                        message: "expected an integer >= 1".into(),
                    }]))
                }
            },
        };
        let migrated_from = if found < SCHEMA_VERSION {
            migrate_document(&mut doc, found, SCHEMA_VERSION).map_err(|m| {
                ConfigError::Invalid(vec![Issue {
                    path: "schema_version".into(),
                    message: m,
                }])
            })?;
            Some(found)
        } else {
            if found > SCHEMA_VERSION {
                cx.warn(
                    "schema_version",
                    format!(
                        "file is version {found}, this build understands {SCHEMA_VERSION}; \
                         unknown settings are kept"
                    ),
                );
            }
            None
        };

        let root: &dyn TableLike = doc.as_table();
        let config = read_config(&mut cx, root);
        if !cx.errors.is_empty() {
            return Err(ConfigError::Invalid(cx.errors));
        }
        Ok(Loaded {
            config,
            warnings: cx.warnings,
            migrated_from,
        })
    }
}

fn read_config(cx: &mut Ctx, root: &dyn TableLike) -> Config {
    let mut cfg = Config::default();
    cx.unknown_keys(
        "",
        Some(root),
        &[
            "schema_version",
            "general",
            "typing",
            "hotkeys",
            "macros",
            "compat",
            "privacy",
            "update",
            "profiles",
        ],
    );

    let t = cx.section(root, "general");
    cx.unknown_keys(
        "general",
        t,
        &[
            "enabled",
            "default_mode",
            "mode_scope",
            "scheme",
            "encoding",
            "ui_language",
            "theme",
            "run_at_login",
            "show_onboarding",
        ],
    );
    let d = General::default();
    cfg.general = General {
        enabled: cx.boolean(t, "general", "enabled", d.enabled),
        default_mode: cx.choice(t, "general", "default_mode", d.default_mode, MODES),
        mode_scope: cx.choice(t, "general", "mode_scope", d.mode_scope, SCOPES),
        scheme: cx.choice(t, "general", "scheme", d.scheme, SCHEMES),
        encoding: cx.choice(t, "general", "encoding", d.encoding, ENCODINGS),
        ui_language: cx.choice(t, "general", "ui_language", d.ui_language, LANGS),
        theme: cx.choice(t, "general", "theme", d.theme, THEMES),
        run_at_login: cx.boolean(t, "general", "run_at_login", d.run_at_login),
        show_onboarding: cx.boolean(t, "general", "show_onboarding", d.show_onboarding),
    };

    let t = cx.section(root, "typing");
    cx.unknown_keys(
        "typing",
        t,
        &[
            "tone_style",
            "free_marking",
            "spell_check",
            "auto_restore_invalid",
            "restore_key",
            "backspace_mode",
            "commit_on_focus_loss",
            "telex_w_standalone",
            "telex_brackets",
            "allow_foreign_onsets",
            "quick_telex",
            "quick_end_consonant",
            "auto_capitalize",
        ],
    );
    let d = Typing::default();
    cfg.typing = Typing {
        tone_style: cx.choice(t, "typing", "tone_style", d.tone_style, TONE_STYLES),
        free_marking: cx.boolean(t, "typing", "free_marking", d.free_marking),
        spell_check: cx.boolean(t, "typing", "spell_check", d.spell_check),
        auto_restore_invalid: cx.boolean(
            t,
            "typing",
            "auto_restore_invalid",
            d.auto_restore_invalid,
        ),
        restore_key: cx.choice(t, "typing", "restore_key", d.restore_key, RESTORE_KEYS),
        backspace_mode: cx.choice(t, "typing", "backspace_mode", d.backspace_mode, BACKSPACES),
        commit_on_focus_loss: cx.boolean(
            t,
            "typing",
            "commit_on_focus_loss",
            d.commit_on_focus_loss,
        ),
        telex_w_standalone: cx.boolean(t, "typing", "telex_w_standalone", d.telex_w_standalone),
        telex_brackets: cx.boolean(t, "typing", "telex_brackets", d.telex_brackets),
        allow_foreign_onsets: cx.boolean(
            t,
            "typing",
            "allow_foreign_onsets",
            d.allow_foreign_onsets,
        ),
        quick_telex: cx.boolean(t, "typing", "quick_telex", d.quick_telex),
        quick_end_consonant: cx.boolean(t, "typing", "quick_end_consonant", d.quick_end_consonant),
        auto_capitalize: cx.boolean(t, "typing", "auto_capitalize", d.auto_capitalize),
    };

    let t = cx.section(root, "hotkeys");
    cx.unknown_keys(
        "hotkeys",
        t,
        &["toggle_mode", "cycle_scheme", "open_settings"],
    );
    let d = Hotkeys::default();
    cfg.hotkeys = Hotkeys {
        toggle_mode: cx.hotkey(t, "hotkeys", "toggle_mode", d.toggle_mode),
        cycle_scheme: cx.hotkey(t, "hotkeys", "cycle_scheme", d.cycle_scheme),
        open_settings: cx.hotkey(t, "hotkeys", "open_settings", d.open_settings),
    };
    let hk = [
        ("toggle_mode", cfg.hotkeys.toggle_mode),
        ("cycle_scheme", cfg.hotkeys.cycle_scheme),
        ("open_settings", cfg.hotkeys.open_settings),
    ];
    for (i, (name, a)) in hk.iter().enumerate() {
        for (other, b) in &hk[i + 1..] {
            if a.is_some() && a == b {
                cx.err(
                    &format!("hotkeys.{name}"),
                    format!("same hotkey as `{other}`"),
                );
            }
        }
    }

    let t = cx.section(root, "macros");
    cx.unknown_keys(
        "macros",
        t,
        &["enabled", "expand_on", "max_expansion_chars"],
    );
    let d = Macros::default();
    let expand_on = match cx.string_list(t, "macros", "expand_on") {
        None => d.expand_on.clone(),
        Some(names) => {
            let mut out = Vec::new();
            for n in names {
                match EXPAND_ON.iter().find(|(k, _)| *k == n) {
                    Some(&(_, v)) if !out.contains(&v) => out.push(v),
                    Some(_) => {}
                    None => cx.err(
                        "macros.expand_on",
                        format!("`{n}` is not valid, expected: space, enter, tab, punctuation"),
                    ),
                }
            }
            out
        }
    };
    let max = match cx.item(t, "max_expansion_chars") {
        None => d.max_expansion_chars,
        Some(item) => match item.as_integer() {
            Some(v) if (1..=65_536).contains(&v) => v as u32,
            _ => {
                cx.err(
                    "macros.max_expansion_chars",
                    "expected an integer from 1 to 65536",
                );
                d.max_expansion_chars
            }
        },
    };
    cfg.macros = Macros {
        enabled: cx.boolean(t, "macros", "enabled", d.enabled),
        expand_on,
        max_expansion_chars: max,
    };

    let t = cx.section(root, "compat");
    cx.unknown_keys(
        "compat",
        t,
        &[
            "preedit_underline",
            "windows_hook_fallback",
            "windows_hook_apps",
            "linux_surrounding_text",
        ],
    );
    let d = Compat::default();
    let mut apps = cx
        .string_list(t, "compat", "windows_hook_apps")
        .unwrap_or_default();
    if apps.len() > MAX_HOOK_APPS {
        cx.err(
            "compat.windows_hook_apps",
            format!("at most {MAX_HOOK_APPS} entries"),
        );
        apps.truncate(MAX_HOOK_APPS);
    }
    cfg.compat = Compat {
        preedit_underline: cx.boolean(t, "compat", "preedit_underline", d.preedit_underline),
        windows_hook_fallback: cx.boolean(
            t,
            "compat",
            "windows_hook_fallback",
            d.windows_hook_fallback,
        ),
        windows_hook_apps: apps,
        linux_surrounding_text: cx.boolean(
            t,
            "compat",
            "linux_surrounding_text",
            d.linux_surrounding_text,
        ),
    };

    let t = cx.section(root, "privacy");
    cx.unknown_keys("privacy", t, &["diagnostics_level"]);
    cfg.privacy = Privacy {
        diagnostics_level: cx.choice(
            t,
            "privacy",
            "diagnostics_level",
            DiagnosticsLevel::default(),
            DIAGNOSTICS,
        ),
    };

    let t = cx.section(root, "update");
    cx.unknown_keys("update", t, &["channel", "check_automatically"]);
    cfg.update = Update {
        channel: cx.choice(t, "update", "channel", UpdateChannel::default(), CHANNELS),
        check_automatically: cx.boolean(t, "update", "check_automatically", false),
    };

    cfg.profiles = read_profiles(cx, root);
    cfg
}

fn read_profiles(cx: &mut Ctx, root: &dyn TableLike) -> Vec<Profile> {
    let Some(item) = root.get("profiles") else {
        return Vec::new();
    };
    let tables: Vec<&dyn TableLike> = if let Some(aot) = item.as_array_of_tables() {
        aot.iter().map(|t| t as &dyn TableLike).collect()
    } else if let Some(arr) = item.as_array() {
        arr.iter()
            .filter_map(|v| v.as_inline_table())
            .map(|t| t as &dyn TableLike)
            .collect()
    } else {
        cx.err("profiles", "expected a list of tables ([[profiles]])");
        return Vec::new();
    };

    let mut out: Vec<Profile> = Vec::new();
    for (i, t) in tables.into_iter().enumerate() {
        let sect = format!("profiles[{i}]");
        let t = Some(t);
        cx.unknown_keys(&sect, t, &["id", "name", "enabled", "match", "override"]);

        let id = cx.opt_str(t, &sect, "id").unwrap_or_default();
        if !is_uuid(&id) {
            cx.err(
                &join(&sect, "id"),
                "expected a UUID such as 123e4567-e89b-12d3-a456-426614174000",
            );
        } else if out.iter().any(|p| p.id.eq_ignore_ascii_case(&id)) {
            cx.err(&join(&sect, "id"), "duplicate profile id");
        }
        let name = cx.opt_str(t, &sect, "name").unwrap_or_default();
        let n = name.chars().count();
        if n == 0 || n > MAX_NAME_CHARS {
            cx.err(
                &join(&sect, "name"),
                format!("expected 1 to {MAX_NAME_CHARS} characters"),
            );
        }
        let enabled = cx.boolean(t, &sect, "enabled", true);

        let msect = format!("{sect}.match");
        let m = match t.and_then(|t| t.get("match")).and_then(Item::as_table_like) {
            Some(m) => Some(m),
            None => {
                cx.err(&msect, "missing [profiles.match] table");
                None
            }
        };
        cx.unknown_keys(&msect, m, &["kind", "value", "pattern"]);
        let kind = cx
            .opt_choice(m, &msect, "kind", MATCH_KINDS)
            .unwrap_or(MatchKind::ExeName);
        if m.is_some() && cx.item(m, "kind").is_none() {
            cx.err(&join(&msect, "kind"), "missing");
        }
        let value = cx.opt_str(m, &msect, "value").unwrap_or_default();
        if value.is_empty() || value.chars().count() > MAX_MATCH_VALUE_CHARS {
            cx.err(
                &join(&msect, "value"),
                format!("expected 1 to {MAX_MATCH_VALUE_CHARS} characters"),
            );
        }
        let pattern = cx.choice(m, &msect, "pattern", MatchPattern::Exact, PATTERNS);

        let osect = format!("{sect}.override");
        let o = cx.section_opt(t, "override", &osect);
        cx.unknown_keys(
            &osect,
            o,
            &[
                "mode",
                "scheme",
                "encoding",
                "tone_style",
                "spell_check",
                "macros_enabled",
                "game_mode",
            ],
        );
        let overrides = ProfileOverride {
            mode: cx.opt_choice(o, &osect, "mode", MODES),
            scheme: cx.opt_choice(o, &osect, "scheme", SCHEMES),
            encoding: cx.opt_choice(o, &osect, "encoding", ENCODINGS),
            tone_style: cx.opt_choice(o, &osect, "tone_style", TONE_STYLES),
            spell_check: cx.opt_bool(o, &osect, "spell_check"),
            macros_enabled: cx.opt_bool(o, &osect, "macros_enabled"),
            game_mode: cx.opt_bool(o, &osect, "game_mode"),
        };

        out.push(Profile {
            id,
            name,
            enabled,
            matcher: ProfileMatch {
                kind,
                value,
                pattern,
            },
            overrides,
        });
    }
    out
}

impl Ctx {
    fn section_opt<'a>(&mut self, t: Tbl<'a>, key: &str, path: &str) -> Tbl<'a> {
        match t.and_then(|t| t.get(key)) {
            None => None,
            Some(item) => match item.as_table_like() {
                Some(x) => Some(x),
                None => {
                    self.err(path, "expected a table");
                    None
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Loaded, ConfigError> {
        Config::parse(s)
    }

    fn issues(s: &str) -> Vec<String> {
        match parse(s) {
            Err(ConfigError::Invalid(v)) => v.into_iter().map(|i| i.path).collect(),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn empty_file_gives_defaults() {
        let l = parse("").unwrap();
        assert_eq!(l.config, Config::default());
        assert!(l.warnings.is_empty());
        assert_eq!(l.migrated_from, None);
    }

    #[test]
    fn default_matches_the_engine_default() {
        assert_eq!(
            Config::default().engine_config(),
            ime_core::EngineConfig::default()
        );
        assert_eq!(
            Config::default().hotkeys.toggle_mode.unwrap().to_string(),
            "Ctrl+Shift"
        );
    }

    #[test]
    fn reads_values_and_maps_them_to_the_engine() {
        let l = parse(
            r#"
schema_version = 1
[general]
scheme = "vni"
default_mode = "en"
[typing]
tone_style = "traditional"
backspace_mode = "raw_key"
restore_key = "none"
[hotkeys]
toggle_mode = "Alt+Z"
cycle_scheme = ""
"#,
        )
        .unwrap();
        let e = l.config.engine_config();
        assert_eq!(e.scheme, Scheme::Vni);
        assert_eq!(e.default_mode, InputMode::English);
        assert_eq!(e.tone_style, ToneStyle::Traditional);
        assert_eq!(e.backspace_mode, BackspaceMode::RawKey);
        assert_eq!(e.restore_key, RestoreKey::None);
        assert_eq!(l.config.hotkeys.toggle_mode.unwrap().to_string(), "Alt+Z");
        assert_eq!(l.config.hotkeys.cycle_scheme, None);
    }

    #[test]
    fn syntax_errors_are_reported() {
        assert!(matches!(parse("[general"), Err(ConfigError::Syntax(_))));
    }

    #[test]
    fn every_invalid_value_is_reported_with_its_path() {
        let got = issues(
            r#"
[general]
scheme = "dvorak"
enabled = "yes"
[typing]
tone_style = 3
[hotkeys]
toggle_mode = "Ctrl+Nope"
[macros]
expand_on = ["space", "bogus"]
max_expansion_chars = 0
[privacy]
diagnostics_level = "everything"
"#,
        );
        for p in [
            "general.scheme",
            "general.enabled",
            "typing.tone_style",
            "hotkeys.toggle_mode",
            "macros.expand_on",
            "macros.max_expansion_chars",
            "privacy.diagnostics_level",
        ] {
            assert!(got.iter().any(|g| g == p), "{p} missing in {got:?}");
        }
    }

    #[test]
    fn legacy_encodings_are_not_accepted_yet() {
        assert_eq!(
            issues("[general]\nencoding = \"tcvn3\"\n"),
            ["general.encoding"]
        );
    }

    #[test]
    fn duplicate_hotkeys_are_rejected() {
        let got = issues("[hotkeys]\ntoggle_mode = \"Alt+Z\"\ncycle_scheme = \"alt+z\"\n");
        assert_eq!(got, ["hotkeys.toggle_mode"]);
    }

    #[test]
    fn schema_version_rules() {
        assert_eq!(issues("schema_version = 0\n"), ["schema_version"]);
        assert_eq!(issues("schema_version = \"1\"\n"), ["schema_version"]);
        let l = parse("schema_version = 7\n[general]\nfuture = 1\n").unwrap();
        assert!(l.warnings.iter().any(|w| w.path == "schema_version"));
        assert!(l.warnings.iter().any(|w| w.path == "general.future"));
    }

    const PROFILE: &str = r#"
[[profiles]]
id = "123e4567-e89b-12d3-a456-426614174000"
name = "VS Code"
[profiles.match]
kind = "exe_name"
value = "code.exe"
[profiles.override]
mode = "en"
game_mode = false
"#;

    #[test]
    fn reads_profiles() {
        let l = parse(PROFILE).unwrap();
        let p = &l.config.profiles[0];
        assert_eq!(p.name, "VS Code");
        assert!(p.enabled);
        assert_eq!(p.matcher.kind, MatchKind::ExeName);
        assert_eq!(p.matcher.pattern, MatchPattern::Exact);
        assert_eq!(p.overrides.mode, Some(InputMode::English));
        assert_eq!(p.overrides.game_mode, Some(false));
        assert_eq!(p.overrides.scheme, None);
    }

    #[test]
    fn profile_validation() {
        let dup = format!("{PROFILE}{PROFILE}");
        assert_eq!(issues(&dup), ["profiles[1].id"]);
        let bad = r#"
[[profiles]]
id = "not-a-uuid"
name = ""
[profiles.match]
kind = "window_title"
value = ""
"#;
        let got = issues(bad);
        for p in [
            "profiles[0].id",
            "profiles[0].name",
            "profiles[0].match.kind",
            "profiles[0].match.value",
        ] {
            assert!(got.iter().any(|g| g == p), "{p} missing in {got:?}");
        }
        assert!(issues(
            "[[profiles]]\nid = \"123e4567-e89b-12d3-a456-426614174000\"\nname = \"x\"\n"
        )
        .contains(&"profiles[0].match".to_string()));
    }
}
