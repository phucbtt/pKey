//! VietIME core engine: pure, deterministic, OS-agnostic (spec §7).
//!
//! Adapters translate native key events into [`KeyInput`], call [`Engine::process`] and apply the
//! returned [`EngineAction`]s to the host's composition API.
#![forbid(unsafe_code)]

mod composition;
mod config;
mod engine;
mod types;

pub use config::{BackspaceMode, EngineConfig, RestoreKey};
pub use engine::{Engine, MAX_COMPOSITION};
pub use ime_unicode::Caret;
pub use ime_vietnamese::ToneStyle;
pub use types::{
    Capabilities, ContextId, ContextInfo, EngineAction, InputMode, KeyFlags, KeyInput, LogicalKey,
    Modifiers, Notify, Output, OutputEncoding, Scheme,
};
