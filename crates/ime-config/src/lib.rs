//! pKey configuration: schema v1, validation, migration, lossless TOML round-trip, atomic save,
//! and per-application profile resolution (spec §12, §16; CORE-008, CORE-012).
//!
//! Unknown keys written by a newer version are preserved when the file is saved again, because
//! saving edits the existing TOML document instead of regenerating it.
#![forbid(unsafe_code)]

mod io;
mod migrate;
mod parse;
mod paths;
mod profile;
mod render;
mod types;

pub use io::{load_file, load_or_recover, save_atomic, Recovered, BACKUP_COUNT};
pub use migrate::{migrate_document, Migration, MIGRATIONS};
pub use parse::{Loaded, SCHEMA_VERSION};
pub use paths::Paths;
pub use profile::{AppIdentity, Effective};
pub use types::{
    Compat, Config, ConfigError, DiagnosticsLevel, ExpandOn, General, Hotkeys, Issue, Macros,
    MatchKind, MatchPattern, ModeScope, Privacy, Profile, ProfileMatch, ProfileOverride, Theme,
    Typing, UiLanguage, Update, UpdateChannel,
};
