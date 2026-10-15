//! Optional diagnostic trace, for finding out what a client really does with the engine's
//! output. It never records what is typed: only event kinds, lengths and counts.
//!
//! On when `$XDG_STATE_HOME/pkey/trace.enable` exists (or `PKEY_IBUS_TRACE=<file>` is set);
//! written to `$XDG_STATE_HOME/pkey/ibus-trace.log` and cut when it passes 1 MB.

use std::fmt::Arguments;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

const MAX_BYTES: u64 = 1 << 20;

pub struct Trace {
    out: Option<Mutex<File>>,
    start: Instant,
}

fn state_dir() -> Option<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    var("XDG_STATE_HOME")
        .or_else(|| var("HOME").map(|h| h.join(".local/state")))
        .map(|d| d.join("pkey"))
}

impl Trace {
    pub fn off() -> Trace {
        Trace {
            out: None,
            start: Instant::now(),
        }
    }

    pub fn from_environment() -> Trace {
        let path = std::env::var_os("PKEY_IBUS_TRACE")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                let dir = state_dir()?;
                dir.join("trace.enable")
                    .exists()
                    .then(|| dir.join("ibus-trace.log"))
            });
        let Some(path) = path else {
            return Trace::off();
        };
        let too_big = std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES);
        let mut options = OpenOptions::new();
        options.create(true).write(true);
        if too_big {
            options.truncate(true);
        } else {
            options.append(true);
        }
        Trace {
            out: options.open(path).ok().map(Mutex::new),
            start: Instant::now(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.out.is_some()
    }

    pub fn log(&self, args: Arguments<'_>) {
        let Some(out) = &self.out else { return };
        if let Ok(mut f) = out.lock() {
            let _ = writeln!(f, "{:>9.3} {}", self.start.elapsed().as_secs_f64(), args);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_by_default_and_writes_lines() {
        let t = Trace::off();
        assert!(!t.enabled());
        t.log(format_args!("ignored"));

        let path = std::env::temp_dir().join(format!("pkey-trace-{}", std::process::id()));
        let t = Trace {
            out: Some(Mutex::new(File::create(&path).unwrap())),
            start: Instant::now(),
        };
        t.log(format_args!("key kind=char n={}", 3));
        let s = std::fs::read_to_string(&path).unwrap();
        assert!(s.contains("key kind=char n=3"));
        std::fs::remove_file(path).ok();
    }
}
