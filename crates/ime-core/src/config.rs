use ime_vietnamese::ToneStyle;

use crate::types::{InputMode, OutputEncoding, Scheme};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RestoreKey {
    /// Esc commits the raw keys the user typed.
    #[default]
    Esc,
    /// Esc is not special: it commits the word and passes through.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BackspaceMode {
    /// Remove the last letter of the displayed word.
    #[default]
    Grapheme,
    /// Undo the last typed key and replay the rest.
    RawKey,
}

/// Typing options the engine needs (spec §16.2 `[general]` / `[typing]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineConfig {
    pub default_mode: InputMode,
    pub scheme: Scheme,
    pub encoding: OutputEncoding,
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
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            default_mode: InputMode::Vietnamese,
            scheme: Scheme::Telex,
            encoding: OutputEncoding::UnicodeNfc,
            tone_style: ToneStyle::Modern,
            free_marking: true,
            spell_check: true,
            auto_restore_invalid: true,
            restore_key: RestoreKey::Esc,
            backspace_mode: BackspaceMode::Grapheme,
            commit_on_focus_loss: true,
            telex_w_standalone: true,
            telex_brackets: false,
            allow_foreign_onsets: false,
        }
    }
}
