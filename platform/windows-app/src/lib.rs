//! pKey standalone app (see `main.rs`). The library exists so tests can drive the real thing.

pub mod controller;
pub mod settings_model;

#[cfg(windows)]
pub mod win;
