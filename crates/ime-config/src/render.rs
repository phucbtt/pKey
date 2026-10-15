//! [`Config`] -> TOML text, editing an existing document so comments, key order and keys this
//! version does not know are preserved (spec §16.4).

use toml_edit::{value, Array, ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::parse::*;
use crate::types::*;

/// Set `key`, keeping the comment/whitespace around an existing value.
fn put(t: &mut Table, key: &str, new: impl Into<Value>) {
    let mut new: Value = new.into();
    if let Some(Item::Value(old)) = t.get_mut(key) {
        *new.decor_mut() = old.decor().clone();
        *old = new;
    } else {
        t[key] = Item::Value(new);
    }
}

fn section<'a>(parent: &'a mut Table, name: &str) -> &'a mut Table {
    if !parent.get(name).is_some_and(Item::is_table) {
        parent[name] = Item::Table(Table::new());
    }
    parent[name].as_table_mut().expect("just ensured a table")
}

fn put_list(t: &mut Table, key: &str, items: impl IntoIterator<Item = String>) {
    let arr: Array = items.into_iter().collect();
    put(t, key, arr);
}

fn hotkey_text(h: &Option<ime_core::Hotkey>) -> String {
    h.map(|h| h.to_string()).unwrap_or_default()
}

impl Config {
    /// Render the configuration. `base` is the current file text, if any; its comments and
    /// unknown keys are kept. An unparsable `base` is ignored.
    pub fn render(&self, base: Option<&str>) -> String {
        let mut doc: DocumentMut = base.and_then(|b| b.parse().ok()).unwrap_or_default();
        let root = doc.as_table_mut();
        put(root, "schema_version", SCHEMA_VERSION);

        let t = section(root, "general");
        let g = &self.general;
        put(t, "enabled", g.enabled);
        put(t, "default_mode", name_of(MODES, g.default_mode));
        put(t, "mode_scope", name_of(SCOPES, g.mode_scope));
        put(t, "scheme", name_of(SCHEMES, g.scheme));
        put(t, "encoding", name_of(ENCODINGS, g.encoding));
        put(t, "ui_language", name_of(LANGS, g.ui_language));
        put(t, "theme", name_of(THEMES, g.theme));
        put(t, "run_at_login", g.run_at_login);
        put(t, "show_onboarding", g.show_onboarding);

        let t = section(root, "typing");
        let y = &self.typing;
        put(t, "tone_style", name_of(TONE_STYLES, y.tone_style));
        put(t, "free_marking", y.free_marking);
        put(t, "spell_check", y.spell_check);
        put(t, "auto_restore_invalid", y.auto_restore_invalid);
        put(t, "restore_key", name_of(RESTORE_KEYS, y.restore_key));
        put(t, "backspace_mode", name_of(BACKSPACES, y.backspace_mode));
        put(t, "commit_on_focus_loss", y.commit_on_focus_loss);
        put(t, "telex_w_standalone", y.telex_w_standalone);
        put(t, "telex_brackets", y.telex_brackets);
        put(t, "allow_foreign_onsets", y.allow_foreign_onsets);
        put(t, "quick_telex", y.quick_telex);
        put(t, "quick_end_consonant", y.quick_end_consonant);
        put(t, "auto_capitalize", y.auto_capitalize);

        let t = section(root, "hotkeys");
        put(t, "toggle_mode", hotkey_text(&self.hotkeys.toggle_mode));
        put(t, "cycle_scheme", hotkey_text(&self.hotkeys.cycle_scheme));
        put(t, "open_settings", hotkey_text(&self.hotkeys.open_settings));

        let t = section(root, "macros");
        put(t, "enabled", self.macros.enabled);
        put_list(
            t,
            "expand_on",
            self.macros
                .expand_on
                .iter()
                .map(|e| name_of(EXPAND_ON, *e).to_string()),
        );
        put(
            t,
            "max_expansion_chars",
            i64::from(self.macros.max_expansion_chars),
        );

        let t = section(root, "compat");
        let c = &self.compat;
        put(t, "preedit_underline", c.preedit_underline);
        put(t, "windows_hook_fallback", c.windows_hook_fallback);
        put_list(t, "windows_hook_apps", c.windows_hook_apps.iter().cloned());
        put(t, "linux_surrounding_text", c.linux_surrounding_text);

        let t = section(root, "privacy");
        put(
            t,
            "diagnostics_level",
            name_of(DIAGNOSTICS, self.privacy.diagnostics_level),
        );

        let t = section(root, "update");
        put(t, "channel", name_of(CHANNELS, self.update.channel));
        put(t, "check_automatically", self.update.check_automatically);

        root.remove("profiles");
        if !self.profiles.is_empty() {
            let mut aot = ArrayOfTables::new();
            for p in &self.profiles {
                aot.push(profile_table(p));
            }
            root["profiles"] = Item::ArrayOfTables(aot);
        }
        doc.to_string()
    }
}

fn profile_table(p: &Profile) -> Table {
    let mut t = Table::new();
    t["id"] = value(p.id.as_str());
    t["name"] = value(p.name.as_str());
    t["enabled"] = value(p.enabled);
    let mut m = Table::new();
    m["kind"] = value(name_of(MATCH_KINDS, p.matcher.kind));
    m["value"] = value(p.matcher.value.as_str());
    m["pattern"] = value(name_of(PATTERNS, p.matcher.pattern));
    t["match"] = Item::Table(m);
    let o = &p.overrides;
    if !o.is_empty() {
        let mut ot = Table::new();
        if let Some(v) = o.mode {
            ot["mode"] = value(name_of(MODES, v));
        }
        if let Some(v) = o.scheme {
            ot["scheme"] = value(name_of(SCHEMES, v));
        }
        if let Some(v) = o.encoding {
            ot["encoding"] = value(name_of(ENCODINGS, v));
        }
        if let Some(v) = o.tone_style {
            ot["tone_style"] = value(name_of(TONE_STYLES, v));
        }
        if let Some(v) = o.spell_check {
            ot["spell_check"] = value(v);
        }
        if let Some(v) = o.macros_enabled {
            ot["macros_enabled"] = value(v);
        }
        if let Some(v) = o.game_mode {
            ot["game_mode"] = value(v);
        }
        t["override"] = Item::Table(ot);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use ime_core::{InputMode, Scheme};

    fn busy_config() -> Config {
        let mut c = Config::default();
        c.general.scheme = Scheme::Viqr;
        c.general.default_mode = InputMode::English;
        c.typing.telex_brackets = true;
        c.typing.tone_style = ime_core::ToneStyle::Traditional;
        c.hotkeys.toggle_mode = Some("Ctrl+Alt+Space".parse().unwrap());
        c.hotkeys.cycle_scheme = Some("Alt+Z".parse().unwrap());
        c.macros.expand_on = vec![ExpandOn::Enter];
        c.macros.max_expansion_chars = 100;
        c.compat.windows_hook_apps = vec!["game.exe".into(), "other.exe".into()];
        c.privacy.diagnostics_level = DiagnosticsLevel::Off;
        c.update.channel = UpdateChannel::Beta;
        c.profiles.push(Profile {
            id: "123e4567-e89b-12d3-a456-426614174000".into(),
            name: "Game \"X\"".into(),
            enabled: false,
            matcher: ProfileMatch {
                kind: MatchKind::ProcessPath,
                value: "C:\\Games\\*.exe".into(),
                pattern: MatchPattern::Glob,
            },
            overrides: ProfileOverride {
                mode: Some(InputMode::English),
                scheme: Some(Scheme::Vni),
                spell_check: Some(false),
                game_mode: Some(true),
                ..Default::default()
            },
        });
        c.profiles.push(Profile {
            id: "223e4567-e89b-12d3-a456-426614174000".into(),
            name: "Plain".into(),
            enabled: true,
            matcher: ProfileMatch {
                kind: MatchKind::BundleId,
                value: "com.example.app".into(),
                pattern: MatchPattern::Exact,
            },
            overrides: ProfileOverride::default(),
        });
        c
    }

    #[test]
    fn render_then_parse_round_trips() {
        for cfg in [Config::default(), busy_config()] {
            let text = cfg.render(None);
            let back = Config::parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
            assert_eq!(back.config, cfg, "{text}");
            assert!(back.warnings.is_empty(), "{:?}", back.warnings);
        }
    }

    #[test]
    fn rendering_twice_is_stable() {
        let cfg = busy_config();
        let once = cfg.render(None);
        assert_eq!(cfg.render(Some(&once)), once);
    }

    #[test]
    fn comments_and_unknown_keys_survive_an_edit() {
        let base = "\
# my notes
schema_version = 1
future_top = 42

[general]
scheme = \"telex\"   # keep telex? no, VNI
future_general = \"x\"

[experimental]
flag = true
";
        let mut cfg = Config::parse(base).unwrap().config;
        cfg.general.scheme = Scheme::Vni;
        let out = cfg.render(Some(base));
        assert!(out.contains("# my notes"), "{out}");
        assert!(out.contains("# keep telex? no, VNI"), "{out}");
        assert!(out.contains("future_top = 42"), "{out}");
        assert!(out.contains("future_general = \"x\""), "{out}");
        assert!(out.contains("[experimental]\nflag = true"), "{out}");
        assert!(out.contains("scheme = \"vni\""), "{out}");
        let back = Config::parse(&out).unwrap();
        assert_eq!(back.config.general.scheme, Scheme::Vni);
    }

    #[test]
    fn removing_all_profiles_removes_the_table() {
        let with = busy_config().render(None);
        let mut cfg = busy_config();
        cfg.profiles.clear();
        let out = cfg.render(Some(&with));
        assert!(!out.contains("profiles"), "{out}");
    }

    #[test]
    fn unparsable_base_is_ignored() {
        let out = Config::default().render(Some("[general"));
        assert!(Config::parse(&out).is_ok());
    }
}
