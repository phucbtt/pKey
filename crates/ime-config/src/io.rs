//! Reading and writing `config.toml` (spec §16.4).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::parse::Loaded;
use crate::types::{Config, ConfigError, Issue};

/// Number of rotating backups kept next to the config file.
pub const BACKUP_COUNT: usize = 5;

fn millis_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

pub fn load_file(path: &Path) -> Result<Loaded, ConfigError> {
    let text = fs::read_to_string(path)?;
    Config::parse(&text)
}

/// Outcome of [`load_or_recover`].
#[derive(Debug)]
pub struct Recovered {
    pub config: Config,
    pub warnings: Vec<Issue>,
    /// Set when the file existed but was unusable: where it was moved to.
    pub corrupt_file: Option<PathBuf>,
    /// Why the file was rejected (shown in Settings).
    pub error: Option<String>,
}

/// Load the configuration, never failing: a missing file gives defaults; a broken file is renamed
/// to `config.toml.corrupt-<timestamp>` and defaults are used. Callers that already run with a
/// good configuration should keep it instead of using `config` when `error` is set.
pub fn load_or_recover(path: &Path) -> Recovered {
    match fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Recovered {
            config: Config::default(),
            warnings: Vec::new(),
            corrupt_file: None,
            error: None,
        },
        Err(e) => Recovered {
            config: Config::default(),
            warnings: Vec::new(),
            corrupt_file: None,
            error: Some(format!("cannot read {}: {e}", path.display())),
        },
        Ok(text) => match Config::parse(&text) {
            Ok(l) => Recovered {
                config: l.config,
                warnings: l.warnings,
                corrupt_file: None,
                error: None,
            },
            Err(e) => {
                let mut name = path.as_os_str().to_owned();
                name.push(format!(".corrupt-{}", millis_now()));
                let target = PathBuf::from(name);
                let moved = fs::rename(path, &target).is_ok().then_some(target);
                Recovered {
                    config: Config::default(),
                    warnings: Vec::new(),
                    corrupt_file: moved,
                    error: Some(e.to_string()),
                }
            }
        },
    }
}

/// Write `config` to `path`: existing content is edited (comments and unknown keys kept), the
/// previous file is copied to `backups/`, and the new file replaces the old one atomically
/// (temp file in the same directory, `fsync`, rename).
pub fn save_atomic(path: &Path, config: &Config) -> std::io::Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    let dir = dir.unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;

    let existing = fs::read_to_string(path).ok();
    let text = config.render(existing.as_deref());
    if existing.as_deref() == Some(text.as_str()) {
        return Ok(());
    }

    if let Some(old) = &existing {
        backup(dir, old)?;
    }

    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    {
        let mut f = File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    // On Windows `rename` replaces an existing file (MOVEFILE_REPLACE_EXISTING).
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

fn backup(dir: &Path, old_text: &str) -> std::io::Result<()> {
    let backups = dir.join("backups");
    fs::create_dir_all(&backups)?;
    let mut n = millis_now();
    // Two saves in the same millisecond must not overwrite each other.
    let file = loop {
        let candidate = backups.join(format!("config-{n:016}.toml"));
        if !candidate.exists() {
            break candidate;
        }
        n += 1;
    };
    fs::write(file, old_text)?;

    let mut all: Vec<PathBuf> = fs::read_dir(&backups)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("config-") && n.ends_with(".toml"))
        })
        .collect();
    all.sort();
    let excess = all.len().saturating_sub(BACKUP_COUNT);
    for p in all.into_iter().take(excess) {
        let _ = fs::remove_file(p);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ime_core::Scheme;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn missing_file_gives_defaults() {
        let d = dir();
        let r = load_or_recover(&d.path().join("config.toml"));
        assert_eq!(r.config, Config::default());
        assert!(r.error.is_none() && r.corrupt_file.is_none());
    }

    #[test]
    fn save_then_load() {
        let d = dir();
        let p = d.path().join("nested").join("config.toml");
        let mut cfg = Config::default();
        cfg.general.scheme = Scheme::Vni;
        save_atomic(&p, &cfg).unwrap();
        assert_eq!(load_file(&p).unwrap().config, cfg);
        assert!(!p.with_extension("toml.tmp").exists());
    }

    #[test]
    fn saving_without_changes_creates_no_backup() {
        let d = dir();
        let p = d.path().join("config.toml");
        let cfg = Config::default();
        save_atomic(&p, &cfg).unwrap();
        save_atomic(&p, &cfg).unwrap();
        assert!(!d.path().join("backups").exists());
    }

    #[test]
    fn backups_rotate() {
        let d = dir();
        let p = d.path().join("config.toml");
        let mut cfg = Config::default();
        for i in 0..(BACKUP_COUNT + 4) {
            cfg.macros.max_expansion_chars = 100 + i as u32;
            save_atomic(&p, &cfg).unwrap();
        }
        let n = fs::read_dir(d.path().join("backups")).unwrap().count();
        assert_eq!(n, BACKUP_COUNT);
        // The newest backup holds the previous save.
        let mut names: Vec<_> = fs::read_dir(d.path().join("backups"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        names.sort();
        let newest = fs::read_to_string(names.last().unwrap()).unwrap();
        let prev = Config::parse(&newest).unwrap().config;
        assert_eq!(
            prev.macros.max_expansion_chars,
            100 + (BACKUP_COUNT + 2) as u32
        );
    }

    #[test]
    fn unknown_keys_survive_save() {
        let d = dir();
        let p = d.path().join("config.toml");
        fs::write(&p, "# hi\nschema_version = 1\nfuture = 1\n").unwrap();
        let mut cfg = load_file(&p).unwrap().config;
        cfg.general.scheme = Scheme::Vni;
        save_atomic(&p, &cfg).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        assert!(
            text.contains("# hi") && text.contains("future = 1"),
            "{text}"
        );
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let d = dir();
        let p = d.path().join("config.toml");
        fs::write(&p, "[general\n").unwrap();
        let r = load_or_recover(&p);
        assert_eq!(r.config, Config::default());
        assert!(r.error.is_some());
        let moved = r.corrupt_file.expect("renamed");
        assert!(moved.exists() && !p.exists());
        assert!(moved
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("config.toml.corrupt-"));
    }

    #[test]
    fn invalid_values_also_count_as_corrupt() {
        let d = dir();
        let p = d.path().join("config.toml");
        fs::write(&p, "[general]\nscheme = \"nope\"\n").unwrap();
        let r = load_or_recover(&p);
        assert!(r.error.unwrap().contains("general.scheme"));
        assert!(r.corrupt_file.is_some());
    }
}
