//! pKey core engine: pure, deterministic, OS-agnostic (spec §7).
//!
//! Adapters translate native key events into [`KeyInput`], call [`Engine::process`] and apply the
//! returned [`EngineAction`]s to the host's composition API.
#![forbid(unsafe_code)]

mod composition;
mod config;
mod engine;
mod hotkey;
mod types;

pub use config::{BackspaceMode, EngineConfig, RestoreKey};
pub use engine::{Engine, MAX_COMPOSITION};
pub use hotkey::{Hotkey, HotkeyError, HotkeyMatcher};
pub use ime_macro::{Macro, MacroTable, Scope as MacroScope};
pub use ime_unicode::Caret;
pub use ime_vietnamese::ToneStyle;
pub use types::{
    Capabilities, ContextId, ContextInfo, EngineAction, InputMode, KeyFlags, KeyInput, LogicalKey,
    MacroDelimiters, Modifiers, Notify, Output, OutputEncoding, Scheme,
};
