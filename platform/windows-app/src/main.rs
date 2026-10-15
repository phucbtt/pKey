//! pKey standalone app: a tray program that types Vietnamese into any Windows application
//! by watching the keyboard (a low-level hook), the way EVKey does. No input method has to be
//! added in Windows Settings.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    pkey_app::win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("pKey is for Windows only");
}
