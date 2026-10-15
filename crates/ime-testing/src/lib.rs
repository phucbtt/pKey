//! Test support: a fake text host that applies engine actions with the semantics of a real
//! document, and a golden-file runner (spec §22.2).
#![forbid(unsafe_code)]

pub mod golden;
mod host;

pub use host::{parse_keys, FakeHost, Step};
