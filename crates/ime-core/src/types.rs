use std::fmt;

use bitflags::bitflags;
use ime_unicode::Caret;

pub type ContextId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Vietnamese,
    English,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scheme {
    #[default]
    Telex,
    Vni,
    /// Declared for the config schema; the rules land in phase P4 (CORE-013).
    Viqr,
}

/// Legacy encodings (TCVN3, VNI Windows) arrive with F013 in 1.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OutputEncoding {
    #[default]
    UnicodeNfc,
    UnicodeNfd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalKey {
    /// A key that produces text; the character is in `KeyInput::text`.
    Char,
    Backspace,
    Delete,
    Enter,
    Tab,
    Escape,
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    /// Anything else (function keys, media keys, ...): commits and passes through.
    Other,
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct Modifiers: u16 {
        const SHIFT = 1;
        const CTRL = 2;
        const ALT = 4;
        /// Win / Cmd / Super.
        const META = 8;
        const CAPS_LOCK = 16;
        /// AltGr; adapters on Windows may also report CTRL+ALT, which is then ignored.
        const ALT_GR = 32;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct KeyFlags: u8 {
        const REPEAT = 1;
        const SYNTHETIC = 2;
        const KEY_UP = 4;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct Capabilities: u32 {
        const PREEDIT = 1 << 0;
        const SELECTION_REPLACE = 1 << 1;
        const SURROUNDING_TEXT = 1 << 2;
        const DELETE_SURROUNDING = 1 << 3;
        /// Password / PIN field: the engine passes everything through.
        const SECURE_INPUT = 1 << 4;
        const FOCUS_METADATA = 1 << 5;
        const TERMINAL_HINT = 1 << 6;
    }
}

/// One key event, already mapped through the keyboard layout by the adapter.
///
/// `Debug` is implemented by hand so that typed characters never end up in logs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct KeyInput {
    pub context_id: ContextId,
    pub logical_key: LogicalKey,
    pub text: Option<char>,
    /// Layout-independent key code (HID usage), used for hotkeys.
    pub physical_code: u32,
    pub modifiers: Modifiers,
    pub flags: KeyFlags,
}

impl KeyInput {
    pub fn new(context_id: ContextId, logical_key: LogicalKey) -> Self {
        KeyInput {
            context_id,
            logical_key,
            text: None,
            physical_code: 0,
            modifiers: Modifiers::empty(),
            flags: KeyFlags::empty(),
        }
    }

    /// A printable character; a space becomes `LogicalKey::Space`.
    pub fn character(context_id: ContextId, ch: char) -> Self {
        let logical_key = if ch == ' ' {
            LogicalKey::Space
        } else {
            LogicalKey::Char
        };
        KeyInput {
            text: Some(ch),
            ..KeyInput::new(context_id, logical_key)
        }
    }
}

impl fmt::Debug for KeyInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyInput")
            .field("context_id", &self.context_id)
            .field("logical_key", &self.logical_key)
            .field("text", &self.text.map(|_| "<redacted>"))
            .field("modifiers", &self.modifiers)
            .field("flags", &self.flags)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum EngineAction {
    UpdatePreedit { text: String, caret: Caret },
    Commit(String),
    /// Remove the preedit without committing anything.
    CancelComposition,
    ModeChanged(InputMode),
}

impl fmt::Debug for EngineAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineAction::UpdatePreedit { text, caret } => {
                write!(f, "UpdatePreedit(<redacted len={}>, {caret:?})", text.len())
            }
            EngineAction::Commit(t) => write!(f, "Commit(<redacted len={}>)", t.len()),
            EngineAction::CancelComposition => f.write_str("CancelComposition"),
            EngineAction::ModeChanged(m) => write!(f, "ModeChanged({m:?})"),
        }
    }
}

/// Result of processing one event. When `consumed` is false the adapter hands the key back to
/// the application *after* applying `actions` (e.g. commit the word, then let the space through).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub consumed: bool,
    pub actions: Vec<EngineAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notify {
    FocusIn { caps: Capabilities },
    FocusOut,
    CaretMoved,
    SelectionChanged,
    /// The host or OS ended our composition: drop local state, emit nothing.
    CompositionTerminatedByHost,
    /// An action could not be applied by the host: drop local state, emit nothing.
    HostCommitFailed,
    Reset,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ContextInfo {
    pub caps: Capabilities,
}
