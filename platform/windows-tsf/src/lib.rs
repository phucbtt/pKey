//! pKey text service for Windows (TSF, spec §14.1).
//!
//! `keymap` is platform independent and tested everywhere; the COM text service itself exists
//! only on Windows.
#![cfg_attr(not(windows), allow(dead_code))]

// `test-hooks` makes `ActivateEx` ignore a refused key-sink registration. Only debug builds (what
// the TSF tests use) may have it; a shipped DLL must not.
#[cfg(all(feature = "test-hooks", not(debug_assertions)))]
compile_error!(
    "the `test-hooks` feature must not be built without debug assertions (release, dist and \
     installer builds); use a debug build, or add `-C debug-assertions=on` to RUSTFLAGS for a \
     release-mode test run"
);

pub mod keymap;

#[cfg(windows)]
mod com;
