//! Where the user's files live (spec §16.1).

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub macros_file: PathBuf,
    /// Snapshots, state, logs. Not user-edited.
    pub runtime_dir: PathBuf,
}

/// Move a folder written under the old product name (VietIME) to its pKey name, once. Does
/// nothing when the new folder already exists or the old one does not. Returns whether it moved.
pub fn migrate_dir(legacy: &Path, new: &Path) -> bool {
    if new.exists() || !legacy.is_dir() {
        return false;
    }
    if let Some(parent) = new.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    std::fs::rename(legacy, new).is_ok()
}

impl Paths {
    /// Everything under `config_dir` (config) and `runtime_dir`.
    pub fn in_dirs(config_dir: PathBuf, runtime_dir: PathBuf) -> Paths {
        Paths {
            config_file: config_dir.join("config.toml"),
            macros_file: config_dir.join("macros.json"),
            config_dir,
            runtime_dir,
        }
    }

    /// The per-user locations for this OS, and where they were before the product was called
    /// pKey: `(current, legacy_config_dir, legacy_runtime_dir)`.
    fn locations() -> Option<(Paths, PathBuf, PathBuf)> {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        if cfg!(windows) {
            let roaming = var("APPDATA")?;
            let local = var("LOCALAPPDATA")?;
            Some((
                Paths::in_dirs(roaming.join("pKey"), local.join("pKey").join("runtime")),
                roaming.join("VietIME"),
                local.join("VietIME").join("runtime"),
            ))
        } else if cfg!(target_os = "macos") {
            let home = var("HOME")?;
            Some((
                Paths::in_dirs(
                    home.join("Library/Application Support/pKey"),
                    home.join("Library/Caches/pKey"),
                ),
                home.join("Library/Application Support/VietIME"),
                home.join("Library/Caches/VietIME"),
            ))
        } else {
            let home = var("HOME");
            let config =
                var("XDG_CONFIG_HOME").or_else(|| home.clone().map(|h| h.join(".config")))?;
            let state = var("XDG_STATE_HOME").or_else(|| home.map(|h| h.join(".local/state")))?;
            Some((
                Paths::in_dirs(config.join("pkey"), state.join("pkey")),
                config.join("vietime"),
                state.join("vietime"),
            ))
        }
    }

    /// The per-user locations for this OS. `PKEY_CONFIG_DIR` overrides the config directory
    /// (portable installs, tests); the runtime directory then becomes `<dir>/runtime`. The old
    /// variable name `VIETIME_CONFIG_DIR` is still honoured.
    ///
    /// Settings saved under the old product name (VietIME) are moved to the new folder the first
    /// time this runs, so nothing is lost by the rename.
    pub fn for_current_user() -> Option<Paths> {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        if let Some(dir) = var("PKEY_CONFIG_DIR").or_else(|| var("VIETIME_CONFIG_DIR")) {
            let runtime = dir.join("runtime");
            return Some(Paths::in_dirs(dir, runtime));
        }
        let (paths, legacy_config, legacy_runtime) = Paths::locations()?;
        migrate_dir(&legacy_config, &paths.config_dir);
        migrate_dir(&legacy_runtime, &paths.runtime_dir);
        Some(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_sit_in_the_config_dir() {
        let p = Paths::in_dirs("/c".into(), "/r".into());
        assert_eq!(p.config_file, PathBuf::from("/c").join("config.toml"));
        assert_eq!(p.macros_file, PathBuf::from("/c").join("macros.json"));
        assert_eq!(p.runtime_dir, PathBuf::from("/r"));
    }

    #[test]
    fn an_old_folder_is_moved_once_with_its_contents() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("VietIME");
        let new = root.path().join("pKey");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("config.toml"), "x = 1").unwrap();
        std::fs::write(old.join("macros.json"), "{}").unwrap();

        assert!(migrate_dir(&old, &new));
        assert!(!old.exists());
        assert_eq!(
            std::fs::read_to_string(new.join("config.toml")).unwrap(),
            "x = 1"
        );
        assert!(new.join("macros.json").exists());
        // A second run has nothing to do.
        assert!(!migrate_dir(&old, &new));
    }

    #[test]
    fn an_existing_new_folder_is_never_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("VietIME");
        let new = root.path().join("pKey");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("config.toml"), "old").unwrap();
        std::fs::write(new.join("config.toml"), "new").unwrap();
        assert!(!migrate_dir(&old, &new));
        assert_eq!(
            std::fs::read_to_string(new.join("config.toml")).unwrap(),
            "new"
        );
        assert!(
            old.join("config.toml").exists(),
            "the old folder is left alone"
        );
    }

    #[test]
    fn nothing_to_migrate_is_fine() {
        let root = tempfile::tempdir().unwrap();
        assert!(!migrate_dir(
            &root.path().join("none"),
            &root.path().join("pKey")
        ));
        assert!(!root.path().join("pKey").exists());
    }
}
