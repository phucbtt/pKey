//! pKey text service for Windows (TSF, spec §14.1).
//!
//! `keymap` is platform independent and tested everywhere; the COM text service itself exists
//! only on Windows.
#![cfg_attr(not(windows), allow(dead_code))]

pub mod keymap;

#[cfg(windows)]
mod com;
