use std::fmt;

use ime_core::{
    BackspaceMode, EngineConfig, Hotkey, InputMode, OutputEncoding, RestoreKey, Scheme, ToneStyle,
};

/// One problem found while reading a configuration (`path` is e.g. `general.scheme`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

#[derive(Debug)]
pub enum ConfigError {
    /// The file is not valid TOML.
    Syntax(String),
    /// Valid TOML, but one or more values are not acceptable.
    Invalid(Vec<Issue>),
    Io(std::io::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Syntax(e) => write!(f, "config syntax error: {e}"),
            ConfigError::Invalid(issues) => {
                f.write_str("invalid config:")?;
                for i in issues {
                    write!(f, "\n  {i}")?;
                }
                Ok(())
            }
            ConfigError::Io(e) => write!(f, "config I/O error: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ModeScope {
    #[default]
    Global,
    PerApp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UiLanguage {
    #[default]
    System,
    Vi,
    En,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DiagnosticsLevel {
    Off,
    #[default]
    Basic,
    Verbose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpandOn {
    Space,
    Enter,
    Tab,
    Punctuation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchKind {
    ExeName,
    ProcessPath,
    Aumid,
    BundleId,
    Program,
    IbusClient,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MatchPattern {
    #[default]
    Exact,
    Glob,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct General {
    pub enabled: bool,
    pub default_mode: InputMode,
    pub mode_scope: ModeScope,
    pub scheme: Scheme,
    pub encoding: OutputEncoding,
    pub ui_language: UiLanguage,
    pub theme: Theme,
    pub run_at_login: bool,
    pub show_onboarding: bool,
}

impl Default for General {
    fn default() -> Self {
        General {
            enabled: true,
            default_mode: InputMode::Vietnamese,
            mode_scope: ModeScope::Global,
            scheme: Scheme::Telex,
            encoding: OutputEncoding::UnicodeNfc,
            ui_language: UiLanguage::System,
            theme: Theme::System,
            run_at_login: true,
            show_onboarding: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Typing {
    pub tone_style: ToneStyle,
    pub free_marking: bool,
    pub spell_check: bool,
    pub auto_restore_invalid: bool,
    pub restore_key: RestoreKey,
    pub backspace_mode: BackspaceMode,
    pub commit_on_focus_loss: bool,
    pub telex_w_standalone: bool,
    pub telex_brackets: bool,
    pub allow_foreign_onsets: bool,
    /// 1.1 features: stored and validated now, not used by the engine yet.
    pub quick_telex: bool,
    pub quick_end_consonant: bool,
    pub auto_capitalize: bool,
}

impl Default for Typing {
    fn default() -> Self {
        let e = EngineConfig::default();
        Typing {
            tone_style: e.tone_style,
            free_marking: e.free_marking,
            spell_check: e.spell_check,
            auto_restore_invalid: e.auto_restore_invalid,
            restore_key: e.restore_key,
            backspace_mode: e.backspace_mode,
            commit_on_focus_loss: e.commit_on_focus_loss,
            telex_w_standalone: e.telex_w_standalone,
            telex_brackets: e.telex_brackets,
            allow_foreign_onsets: e.allow_foreign_onsets,
            quick_telex: false,
            quick_end_consonant: false,
            auto_capitalize: false,
        }
    }
}

/// `None` means the action has no hotkey (empty string in the file).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hotkeys {
    pub toggle_mode: Option<Hotkey>,
    pub cycle_scheme: Option<Hotkey>,
    pub open_settings: Option<Hotkey>,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Hotkeys {
            toggle_mode: "Ctrl+Shift".parse().ok(),
            cycle_scheme: None,
            open_settings: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Macros {
    pub enabled: bool,
    pub expand_on: Vec<ExpandOn>,
    pub max_expansion_chars: u32,
}

impl Default for Macros {
    fn default() -> Self {
        Macros {
            enabled: true,
            expand_on: vec![
                ExpandOn::Space,
                ExpandOn::Enter,
                ExpandOn::Tab,
                ExpandOn::Punctuation,
            ],
            max_expansion_chars: 4096,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compat {
    pub preedit_underline: bool,
    pub windows_hook_fallback: bool,
    pub windows_hook_apps: Vec<String>,
    pub linux_surrounding_text: bool,
    /// Linux (IBus): show the word being typed as preedit. Desktops draw preedit underlined; with
    /// `false` the word is typed straight into the text field instead (needs surrounding text).
    pub linux_preedit: bool,
}

impl Default for Compat {
    fn default() -> Self {
        Compat {
            preedit_underline: true,
            windows_hook_fallback: false,
            windows_hook_apps: Vec::new(),
            linux_surrounding_text: false,
            linux_preedit: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Privacy {
    pub diagnostics_level: DiagnosticsLevel,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Update {
    pub channel: UpdateChannel,
    pub check_automatically: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileMatch {
    pub kind: MatchKind,
    pub value: String,
    pub pattern: MatchPattern,
}

/// Every field is optional: `None` keeps the global value.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ProfileOverride {
    pub mode: Option<InputMode>,
    pub scheme: Option<Scheme>,
    pub encoding: Option<OutputEncoding>,
    pub tone_style: Option<ToneStyle>,
    pub spell_check: Option<bool>,
    pub macros_enabled: Option<bool>,
    pub game_mode: Option<bool>,
}

impl ProfileOverride {
    pub fn is_empty(&self) -> bool {
        *self == ProfileOverride::default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub matcher: ProfileMatch,
    pub overrides: ProfileOverride,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Config {
    pub general: General,
    pub typing: Typing,
    pub hotkeys: Hotkeys,
    pub macros: Macros,
    pub compat: Compat,
    pub privacy: Privacy,
    pub update: Update,
    pub profiles: Vec<Profile>,
}

impl Config {
    /// Delimiters for `Engine::set_macros`.
    pub fn macro_delimiters(&self) -> ime_core::MacroDelimiters {
        use ime_core::MacroDelimiters as D;
        self.macros.expand_on.iter().fold(D::empty(), |acc, e| {
            acc | match e {
                ExpandOn::Space => D::SPACE,
                ExpandOn::Enter => D::ENTER,
                ExpandOn::Tab => D::TAB,
                ExpandOn::Punctuation => D::PUNCTUATION,
            }
        })
    }

    /// The typing options the engine needs, before any per-application override.
    pub fn engine_config(&self) -> EngineConfig {
        EngineConfig {
            default_mode: self.general.default_mode,
            scheme: self.general.scheme,
            encoding: self.general.encoding,
            tone_style: self.typing.tone_style,
            free_marking: self.typing.free_marking,
            spell_check: self.typing.spell_check,
            auto_restore_invalid: self.typing.auto_restore_invalid,
            restore_key: self.typing.restore_key,
            backspace_mode: self.typing.backspace_mode,
            commit_on_focus_loss: self.typing.commit_on_focus_loss,
            telex_w_standalone: self.typing.telex_w_standalone,
            telex_brackets: self.typing.telex_brackets,
            allow_foreign_onsets: self.typing.allow_foreign_onsets,
        }
    }
}
