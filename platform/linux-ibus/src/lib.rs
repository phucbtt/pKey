//! pKey engine for IBus (spec §14.4): talks the IBus engine protocol over D-Bus in pure Rust, so
//! the binary needs neither libibus nor GLib and runs on any IBus version.
pub mod address;
pub mod dbus;
pub mod keymap;
pub mod session;
pub mod trace;
pub mod wire;

/// Key of the VI/EN indicator property.
pub const MODE_PROPERTY: &str = "InputMode";

/// Name of the component (also its D-Bus name when the daemon starts the engine).
pub const COMPONENT_NAME: &str = "org.pkey.IBus.pKey";
