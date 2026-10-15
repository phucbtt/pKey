//! Finding the IBus bus. `ibus-daemon` runs its own private D-Bus; `IBUS_ADDRESS` or a file under
//! `~/.config/ibus/bus` says where. The file name depends on the IBus version and the session
//! (X11 or Wayland), so the directory is scanned instead of guessing the name.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The `IBUS_ADDRESS=` line of a bus file.
pub fn parse_address_file(content: &str) -> Option<String> {
    content
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .find_map(|l| l.trim().strip_prefix("IBUS_ADDRESS="))
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
}

/// The session the files are named after: `unix-wayland-0`, `unix-0` (for `DISPLAY=:0`).
fn session_suffixes(wayland: Option<&str>, display: Option<&str>) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(w) = wayland.filter(|w| !w.is_empty()) {
        v.push(format!("-{w}"));
    }
    if let Some(d) = display.filter(|d| !d.is_empty()) {
        let n = d.rsplit(':').next().unwrap_or(d);
        let n = n.split('.').next().unwrap_or(n);
        v.push(format!("-{n}"));
    }
    v
}

/// The newest bus file, preferring the one of the current session.
pub fn find_in_dir(dir: &Path, wayland: Option<&str>, display: Option<&str>) -> Option<String> {
    let suffixes = session_suffixes(wayland, display);
    let mut found: Vec<(bool, SystemTime, String)> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(addr) = parse_address_file(&content) else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let mine = suffixes.iter().any(|s| name.ends_with(s.as_str()));
        let when = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        found.push((mine, when, addr));
    }
    found.sort();
    found.pop().map(|(_, _, a)| a)
}

fn bus_dir() -> Option<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let config = var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config")))?;
    Some(config.join("ibus").join("bus"))
}

/// Address of the running IBus bus.
pub fn find() -> Option<String> {
    if let Some(a) = std::env::var("IBUS_ADDRESS").ok().filter(|a| !a.is_empty()) {
        return Some(a);
    }
    find_in_dir(
        &bus_dir()?,
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        std::env::var("DISPLAY").ok().as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_address_line() {
        let f = "# comment\n# IBUS_ADDRESS=unix:path=/wrong\nIBUS_ADDRESS=unix:path=/run/x,guid=ab\nIBUS_DAEMON_PID=12\n";
        assert_eq!(
            parse_address_file(f).as_deref(),
            Some("unix:path=/run/x,guid=ab")
        );
        assert_eq!(parse_address_file("IBUS_DAEMON_PID=1\n"), None);
    }

    #[test]
    fn display_suffixes() {
        assert_eq!(
            session_suffixes(Some("wayland-0"), Some(":1.0")),
            vec!["-wayland-0".to_string(), "-1".to_string()]
        );
        assert!(session_suffixes(None, None).is_empty());
    }

    #[test]
    fn prefers_the_current_session() {
        let dir = std::env::temp_dir().join(format!("pkey-addr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("m-unix-wayland-0"), "IBUS_ADDRESS=unix:path=/a\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("m-unix-0"), "IBUS_ADDRESS=unix:path=/b\n").unwrap();
        let got = find_in_dir(&dir, Some("wayland-0"), None);
        assert_eq!(got.as_deref(), Some("unix:path=/a"));
        let got = find_in_dir(&dir, None, Some(":0"));
        assert_eq!(got.as_deref(), Some("unix:path=/b"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
