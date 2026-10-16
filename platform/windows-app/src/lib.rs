//! pKey standalone app (see `main.rs`). The library exists so tests can drive the real thing.

// `test-hooks` makes the keyboard hook accept keys injected with a magic `dwExtraInfo`, which
// defeats the injected-key filter. Only debug builds (what the e2e tests use) may have it.
#[cfg(all(feature = "test-hooks", not(debug_assertions)))]
compile_error!(
    "the `test-hooks` feature weakens the injected-key filter and must not be built without \
     debug assertions (release, dist and installer builds); use a debug build, or add \
     `-C debug-assertions=on` to RUSTFLAGS for a release-mode e2e run"
);

pub mod controller;
pub mod settings_model;

#[cfg(windows)]
pub mod win;
