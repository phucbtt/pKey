//! What the Settings window edits, as plain data. Reading it from the configuration and writing
//! it back is separate from the window, so it can be tested without any UI.

use ime_runtime::ime_config::{
    Config, MatchKind, MatchPattern, Profile, ProfileMatch, ProfileOverride,
};
use ime_runtime::ime_core::{
    BackspaceMode, Hotkey, Modifiers, OutputEncoding, RestoreKey, Scheme, ToneStyle,
};

/// Profiles made by the "Ứng dụng" tab are recognised by this name prefix, so the window never
/// touches profiles the user wrote by hand.
pub const EXCLUDED_PREFIX: &str = "Tắt pKey: ";
/// Profiles made before the rename keep working and are rewritten with the new prefix.
const LEGACY_EXCLUDED_PREFIX: &str = "Tắt VietIME: ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotkeyChoice {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
    /// HID usage of the extra key, `None` for a modifier-only hotkey such as `Ctrl+Shift`.
    pub key: Option<u32>,
}

impl HotkeyChoice {
    pub fn from_hotkey(h: Option<Hotkey>) -> HotkeyChoice {
        let h = h.unwrap_or(Hotkey {
            modifiers: Modifiers::empty(),
            key: None,
        });
        HotkeyChoice {
            ctrl: h.modifiers.contains(Modifiers::CTRL),
            shift: h.modifiers.contains(Modifiers::SHIFT),
            alt: h.modifiers.contains(Modifiers::ALT),
            meta: h.modifiers.contains(Modifiers::META),
            key: h.key,
        }
    }

    /// `Ok(None)` when no modifier is ticked (the hotkey is switched off).
    pub fn to_hotkey(&self) -> Result<Option<Hotkey>, &'static str> {
        let mut m = Modifiers::empty();
        for (on, flag) in [
            (self.ctrl, Modifiers::CTRL),
            (self.shift, Modifiers::SHIFT),
            (self.alt, Modifiers::ALT),
            (self.meta, Modifiers::META),
        ] {
            if on {
                m |= flag;
            }
        }
        if m.is_empty() {
            return if self.key.is_some() {
                Err("Phím tắt cần ít nhất một phím Ctrl, Shift, Alt hoặc Win")
            } else {
                Ok(None)
            };
        }
        Ok(Some(Hotkey {
            modifiers: m,
            key: self.key,
        }))
    }
}

/// The keys offered next to the modifiers: (label, HID usage).
pub fn key_choices() -> Vec<(String, u32)> {
    let mut v = Vec::new();
    let mut add = |name: String| {
        if let Ok(h) = format!("Ctrl+{name}").parse::<Hotkey>() {
            if let Some(code) = h.key {
                v.push((name, code));
            }
        }
    };
    add("Space".into());
    for c in 'A'..='Z' {
        add(c.to_string());
    }
    for d in 0..=9 {
        add(d.to_string());
    }
    for f in 1..=12 {
        add(format!("F{f}"));
    }
    v
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub scheme: Scheme,
    pub nfd_output: bool,
    pub tone_style: ToneStyle,
    pub spell_check: bool,
    pub auto_restore: bool,
    pub free_marking: bool,
    pub esc_restores_raw: bool,
    pub raw_backspace: bool,
    pub telex_w_standalone: bool,
    pub telex_brackets: bool,
    pub foreign_onsets: bool,
    pub macros_enabled: bool,
    pub autostart: bool,
    pub toggle: HotkeyChoice,
    /// Executable names in which pKey does nothing at all.
    pub excluded: Vec<String>,
}

fn is_ours(p: &Profile) -> bool {
    (p.name.starts_with(EXCLUDED_PREFIX) || p.name.starts_with(LEGACY_EXCLUDED_PREFIX))
        && p.matcher.kind == MatchKind::ExeName
        && p.overrides.game_mode == Some(true)
}

/// A stable UUID-shaped id for an excluded program (the same name always gives the same id).
fn id_for(exe: &str) -> String {
    let fnv = |seed: u64| {
        exe.to_ascii_lowercase().bytes().fold(seed, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        })
    };
    let (a, b) = (fnv(0xcbf2_9ce4_8422_2325), fnv(0x9e37_79b9_7f4a_7c15));
    format!(
        "{:08x}-{:04x}-4{:03x}-a{:03x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        a & 0xfff,
        (b >> 48) & 0xfff,
        b & 0xffff_ffff_ffff
    )
}

/// `chrome.exe` from user input: trims, lower-cases is not applied (names keep their case), and a
/// missing `.exe` is added. `None` for something that cannot be a program name.
pub fn normalize_exe(input: &str) -> Option<String> {
    let name = input.trim().trim_matches('"');
    let name = name.rsplit(['\\', '/']).next().unwrap_or(name).trim();
    if name.is_empty() || name.contains(['*', '?', '<', '>', '|', ':']) {
        return None;
    }
    Some(if name.to_ascii_lowercase().ends_with(".exe") {
        name.to_string()
    } else {
        format!("{name}.exe")
    })
}

impl Settings {
    pub fn from_config(c: &Config, autostart: bool) -> Settings {
        Settings {
            scheme: c.general.scheme,
            nfd_output: c.general.encoding == OutputEncoding::UnicodeNfd,
            tone_style: c.typing.tone_style,
            spell_check: c.typing.spell_check,
            auto_restore: c.typing.auto_restore_invalid,
            free_marking: c.typing.free_marking,
            esc_restores_raw: c.typing.restore_key == RestoreKey::Esc,
            raw_backspace: c.typing.backspace_mode == BackspaceMode::RawKey,
            telex_w_standalone: c.typing.telex_w_standalone,
            telex_brackets: c.typing.telex_brackets,
            foreign_onsets: c.typing.allow_foreign_onsets,
            macros_enabled: c.macros.enabled,
            autostart,
            toggle: HotkeyChoice::from_hotkey(c.hotkeys.toggle_mode),
            excluded: c
                .profiles
                .iter()
                .filter(|p| is_ours(p))
                .map(|p| p.matcher.value.clone())
                .collect(),
        }
    }

    /// Write the settings into `c`, leaving everything the window does not edit untouched.
    pub fn apply_to(&self, c: &mut Config) -> Result<(), &'static str> {
        let toggle = self.toggle.to_hotkey()?;
        c.general.scheme = self.scheme;
        c.general.encoding = if self.nfd_output {
            OutputEncoding::UnicodeNfd
        } else {
            OutputEncoding::UnicodeNfc
        };
        c.general.run_at_login = self.autostart;
        c.typing.tone_style = self.tone_style;
        c.typing.spell_check = self.spell_check;
        c.typing.auto_restore_invalid = self.auto_restore;
        c.typing.free_marking = self.free_marking;
        c.typing.restore_key = if self.esc_restores_raw {
            RestoreKey::Esc
        } else {
            RestoreKey::None
        };
        c.typing.backspace_mode = if self.raw_backspace {
            BackspaceMode::RawKey
        } else {
            BackspaceMode::Grapheme
        };
        c.typing.telex_w_standalone = self.telex_w_standalone;
        c.typing.telex_brackets = self.telex_brackets;
        c.typing.allow_foreign_onsets = self.foreign_onsets;
        c.macros.enabled = self.macros_enabled;
        c.hotkeys.toggle_mode = toggle;

        // Our exclusion profiles are rebuilt from the list; the user's own stay as they are.
        c.profiles.retain(|p| !is_ours(p));
        let mut seen: Vec<String> = Vec::new();
        for exe in &self.excluded {
            let key = exe.to_ascii_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            c.profiles.push(Profile {
                id: id_for(exe),
                name: format!("{EXCLUDED_PREFIX}{exe}"),
                enabled: true,
                matcher: ProfileMatch {
                    kind: MatchKind::ExeName,
                    value: exe.clone(),
                    pattern: MatchPattern::Exact,
                },
                overrides: ProfileOverride {
                    game_mode: Some(true),
                    ..ProfileOverride::default()
                },
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings::from_config(&Config::default(), false)
    }

    #[test]
    fn defaults_round_trip_without_changing_the_config() {
        let mut c = Config::default();
        let s = Settings::from_config(&c, c.general.run_at_login);
        s.apply_to(&mut c).unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn every_option_reaches_the_config() {
        let mut s = settings();
        s.scheme = Scheme::Vni;
        s.nfd_output = true;
        s.tone_style = ToneStyle::Traditional;
        s.spell_check = false;
        s.auto_restore = false;
        s.free_marking = false;
        s.esc_restores_raw = false;
        s.raw_backspace = true;
        s.telex_w_standalone = false;
        s.telex_brackets = true;
        s.foreign_onsets = true;
        s.macros_enabled = false;
        s.autostart = true;
        let mut c = Config::default();
        s.apply_to(&mut c).unwrap();
        assert_eq!(Settings::from_config(&c, true), s);
        assert_eq!(c.general.scheme, Scheme::Vni);
        assert_eq!(c.typing.restore_key, RestoreKey::None);
        assert_eq!(c.typing.backspace_mode, BackspaceMode::RawKey);
    }

    #[test]
    fn the_hotkey_choice_round_trips() {
        for text in [
            "Ctrl+Shift",
            "Alt+Z",
            "Ctrl+Alt+Space",
            "Win+F5",
            "Ctrl+Shift+1",
        ] {
            let h: Hotkey = text.parse().unwrap();
            let choice = HotkeyChoice::from_hotkey(Some(h));
            assert_eq!(choice.to_hotkey().unwrap(), Some(h), "{text}");
        }
        let none = HotkeyChoice::from_hotkey(None);
        assert_eq!(none.to_hotkey().unwrap(), None);
        // A key alone would swallow normal typing.
        let bad = HotkeyChoice {
            key: Some(0x04),
            ..none
        };
        assert!(bad.to_hotkey().is_err());
    }

    #[test]
    fn an_invalid_hotkey_leaves_the_config_alone() {
        let mut s = settings();
        s.toggle = HotkeyChoice {
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
            key: Some(0x2C),
        };
        s.scheme = Scheme::Vni;
        let mut c = Config::default();
        assert!(s.apply_to(&mut c).is_err());
        assert_eq!(c, Config::default());
    }

    #[test]
    fn key_choices_cover_letters_digits_function_keys_and_space() {
        let keys = key_choices();
        assert_eq!(keys.len(), 1 + 26 + 10 + 12);
        assert!(keys.iter().any(|(n, c)| n == "Space" && *c == 0x2C));
        assert!(keys.iter().any(|(n, c)| n == "Z" && *c == 0x1D));
        assert!(keys.iter().any(|(n, c)| n == "F12" && *c == 0x45));
    }

    #[test]
    fn excluded_programs_become_profiles_and_come_back() {
        let mut s = settings();
        s.excluded = vec!["game.exe".into(), "Code.exe".into(), "GAME.EXE".into()];
        let mut c = Config::default();
        s.apply_to(&mut c).unwrap();
        assert_eq!(c.profiles.len(), 2, "duplicates (any case) are dropped");
        // The profiles are valid for the config file and do what they say.
        let text = c.render(None);
        let back = Config::parse(&text).unwrap().config;
        assert_eq!(back, c);
        let app = ime_runtime::ime_config::AppIdentity {
            exe_name: Some("game.exe".into()),
            ..Default::default()
        };
        assert!(c.resolve(&app).game_mode);
        assert_eq!(
            Settings::from_config(&c, false).excluded,
            ["game.exe", "Code.exe"]
        );
    }

    #[test]
    fn hand_written_profiles_are_not_touched() {
        let mut c = Config::default();
        c.profiles.push(Profile {
            id: "123e4567-e89b-12d3-a456-426614174000".into(),
            name: "Mine".into(),
            enabled: true,
            matcher: ProfileMatch {
                kind: MatchKind::ExeName,
                value: "mine.exe".into(),
                pattern: MatchPattern::Exact,
            },
            overrides: ProfileOverride {
                game_mode: Some(true),
                ..Default::default()
            },
        });
        let mut s = Settings::from_config(&c, false);
        assert!(s.excluded.is_empty(), "not made by this window");
        s.excluded = vec!["a.exe".into()];
        s.apply_to(&mut c).unwrap();
        assert_eq!(c.profiles.len(), 2);
        s.excluded.clear();
        s.apply_to(&mut c).unwrap();
        assert_eq!(c.profiles.len(), 1);
        assert_eq!(c.profiles[0].name, "Mine");
    }

    #[test]
    fn profiles_made_before_the_rename_are_still_recognised_and_renamed() {
        let mut c = Config::default();
        let mut s = settings();
        s.excluded = vec!["old.exe".into()];
        s.apply_to(&mut c).unwrap();
        c.profiles[0].name = "Tắt VietIME: old.exe".to_string();
        assert_eq!(Settings::from_config(&c, false).excluded, ["old.exe"]);
        s.apply_to(&mut c).unwrap();
        assert_eq!(c.profiles.len(), 1);
        assert!(c.profiles[0].name.starts_with(EXCLUDED_PREFIX));
    }

    #[test]
    fn profile_ids_are_stable_and_distinct() {
        assert_eq!(id_for("a.exe"), id_for("A.EXE"));
        assert_ne!(id_for("a.exe"), id_for("b.exe"));
        let id = id_for("a.exe");
        assert_eq!(id.len(), 36);
        let mut c = Config::default();
        let mut s = settings();
        s.excluded = vec!["a.exe".into()];
        s.apply_to(&mut c).unwrap();
        assert!(
            Config::parse(&c.render(None)).is_ok(),
            "id is a valid UUID shape"
        );
    }

    #[test]
    fn exe_names_are_cleaned_up() {
        assert_eq!(normalize_exe(" chrome ").as_deref(), Some("chrome.exe"));
        assert_eq!(
            normalize_exe("\"C:\\Games\\Foo\\foo.exe\"").as_deref(),
            Some("foo.exe")
        );
        assert_eq!(normalize_exe("Code.EXE").as_deref(), Some("Code.EXE"));
        assert_eq!(normalize_exe(""), None);
        assert_eq!(normalize_exe("a*b"), None);
    }
}
