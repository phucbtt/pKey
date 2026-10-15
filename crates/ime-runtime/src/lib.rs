//! The engine with everything an adapter needs around it: configuration, macros, per-application
//! profiles, the toggle hotkey and live reload of the config files.
//!
//! Adapters (the Windows TIP, the C ABI used by macOS and Fcitx5, IBus) hold one [`Runtime`] per
//! process and call it from a single thread.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use ime_config::{AppIdentity, Config, Paths};
use ime_core::{
    Capabilities, ContextId, ContextInfo, Engine, EngineAction, Hotkey, HotkeyMatcher, InputMode,
    KeyInput, MacroTable, Notify, Output,
};

pub use ime_config;
pub use ime_core;
pub use ime_macro;

/// A hotkey the adapter has to act on (the mode toggle is handled inside the runtime).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    CycleScheme,
    OpenSettings,
}

/// Which hotkeys completed on one key event (see [`Runtime::feed_hotkeys`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HotkeysFired {
    pub toggle: bool,
    pub cycle: bool,
    pub open: bool,
    /// A fired hotkey has a regular key, which must not reach the application.
    pub swallow: bool,
}

impl HotkeysFired {
    pub fn any(&self) -> bool {
        self.toggle || self.cycle || self.open
    }
}

#[derive(Debug, Default)]
pub struct KeyOutcome {
    pub output: Output,
    /// Set when a non-toggle hotkey fired; the key was consumed when it has a regular key.
    pub hotkey: Option<HotkeyAction>,
}

/// File modification times used to notice edits made by Settings or by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stamp {
    config: Option<SystemTime>,
    macros: Option<SystemTime>,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

struct Hotkeys {
    toggle: Option<HotkeyMatcher>,
    cycle: Option<HotkeyMatcher>,
    open: Option<HotkeyMatcher>,
}

impl Hotkeys {
    fn new(c: &Config) -> Hotkeys {
        let m = |h: Option<Hotkey>| h.map(HotkeyMatcher::new);
        Hotkeys {
            toggle: m(c.hotkeys.toggle_mode),
            cycle: m(c.hotkeys.cycle_scheme),
            open: m(c.hotkeys.open_settings),
        }
    }
}

pub struct Runtime {
    engine: Engine,
    /// The one Vietnamese/English state shared by every application (`mode_scope = "global"`).
    global_mode: InputMode,
    /// Remembered mode per application (`mode_scope = "per_app"`), by `AppIdentity::app_key`.
    app_modes: HashMap<String, InputMode>,
    config: Arc<Config>,
    macro_list: Vec<ime_core::Macro>,
    paths: Option<Paths>,
    stamp: Stamp,
    apps: HashMap<ContextId, AppIdentity>,
    hotkeys: Hotkeys,
    /// The last problem seen while loading files. The previous good settings stay active.
    pub last_error: Option<String>,
}

impl Runtime {
    /// A runtime that does not read any file.
    pub fn new(config: Config, macros: Vec<ime_core::Macro>) -> Runtime {
        let config = Arc::new(config);
        let mut rt = Runtime {
            engine: Engine::new(Arc::new(config.engine_config())),
            global_mode: config.general.default_mode,
            app_modes: HashMap::new(),
            hotkeys: Hotkeys::new(&config),
            config,
            macro_list: Vec::new(),
            paths: None,
            stamp: Stamp::default(),
            apps: HashMap::new(),
            last_error: None,
        };
        rt.install_macros(macros);
        rt
    }

    /// Load `config.toml` and `macros.json` from `paths`. Missing files give defaults; broken
    /// ones are reported in `last_error` (a broken config is moved aside, see
    /// [`ime_config::load_or_recover`]).
    pub fn from_paths(paths: Paths) -> Runtime {
        let (config, macros, error, stamp) = load_files(&paths);
        let mut rt = Runtime::new(config, macros);
        rt.stamp = stamp;
        rt.last_error = error;
        rt.paths = Some(paths);
        rt
    }

    /// Per-user default locations, or a defaults-only runtime when they cannot be determined.
    pub fn from_default_paths() -> Runtime {
        match Paths::for_current_user() {
            Some(p) => Runtime::from_paths(p),
            None => Runtime::new(Config::default(), Vec::new()),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn paths(&self) -> Option<&Paths> {
        self.paths.as_ref()
    }

    fn install_macros(&mut self, list: Vec<ime_core::Macro>) {
        let max = self.config.macros.max_expansion_chars as usize;
        match MacroTable::try_new(list.clone(), max) {
            Ok(table) if self.config.macros.enabled && !table.is_empty() => {
                self.engine
                    .set_macros(Some(Arc::new(table)), self.config.macro_delimiters());
                self.macro_list = list;
            }
            Ok(_) => {
                self.engine.set_macros(None, Default::default());
                self.macro_list = list;
            }
            Err(issues) => {
                self.engine.set_macros(None, Default::default());
                self.last_error = Some(format!("{} invalid macro entries", issues.len()));
            }
        }
    }

    /// Replace configuration and macros (e.g. after Settings saved them) and re-resolve every
    /// open context.
    pub fn apply(&mut self, config: Config, macros: Vec<ime_core::Macro>) {
        self.config = Arc::new(config);
        self.hotkeys = Hotkeys::new(&self.config);
        self.engine
            .reload_config(Arc::new(self.config.engine_config()));
        self.install_macros(macros);
        let ids: Vec<ContextId> = self.apps.keys().copied().collect();
        for id in ids {
            self.resolve_context(id, false);
        }
    }

    /// Re-read the files if they changed on disk. Cheap (two `stat` calls); adapters call it on
    /// focus changes. A file that fails to load leaves the running settings untouched.
    /// Returns whether new settings were applied.
    pub fn poll_reload(&mut self) -> bool {
        let Some(paths) = self.paths.clone() else {
            return false;
        };
        let now = Stamp {
            config: mtime(&paths.config_file),
            macros: mtime(&paths.macros_file),
        };
        if now == self.stamp {
            return false;
        }
        self.stamp = now;

        let mut error = None;
        let config = match ime_config::load_file(&paths.config_file) {
            Ok(l) => l.config,
            Err(ime_config::ConfigError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                Config::default()
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
                return false;
            }
        };
        let max = config.macros.max_expansion_chars as usize;
        let macros = match ime_macro::store::load(&paths.macros_file, max) {
            Ok(m) => m,
            Err(e) => {
                error = Some(e.to_string());
                self.macro_list.clone()
            }
        };
        self.last_error = error;
        self.apply(config, macros);
        true
    }

    // ------------------------------------------------------------------ contexts

    /// New input context for the application `app` (what the adapter knows about it).
    pub fn create_context(&mut self, app: AppIdentity, caps: Capabilities) -> ContextId {
        let id = self.engine.create_context(ContextInfo { caps });
        self.apps.insert(id, app);
        self.resolve_context(id, true);
        id
    }

    pub fn destroy_context(&mut self, id: ContextId) {
        self.engine.destroy_context(id);
        self.apps.remove(&id);
    }

    /// Focus-in: the identity may have changed (same context, another window).
    pub fn update_context(
        &mut self,
        id: ContextId,
        app: AppIdentity,
        caps: Capabilities,
    ) -> Output {
        self.apps.insert(id, app);
        let mut out = self.engine.notify(id, Notify::FocusIn { caps });
        self.resolve_context(id, false);
        // Catch up with a toggle made while another application had focus.
        let synced = self.sync_mode(id);
        out.actions.extend(synced.actions);
        out
    }

    fn start_mode(&self, app: &AppIdentity) -> InputMode {
        match self.config.general.mode_scope {
            ime_config::ModeScope::Global => self.global_mode,
            ime_config::ModeScope::PerApp => app
                .app_key()
                .and_then(|k| self.app_modes.get(k).copied())
                .unwrap_or(self.config.general.default_mode),
        }
    }

    /// Apply the matching profile to a context. `initial` also sets the starting mode.
    fn resolve_context(&mut self, id: ContextId, initial: bool) {
        let Some(app) = self.apps.get(&id) else {
            return;
        };
        let eff = self.config.resolve(app);
        let has_profile = eff.profile_id.is_some();
        self.engine
            .set_context_macros(id, eff.macros_enabled, eff.profile_id.as_deref());
        self.engine.set_context_config(
            id,
            if has_profile {
                Some(Arc::new(eff.engine.clone()))
            } else {
                None
            },
        );
        if initial || eff.game_mode {
            let mode = if eff.game_mode || has_mode_override(&self.config, app) {
                eff.engine.default_mode
            } else {
                self.start_mode(app)
            };
            if self.engine.mode(id) != mode {
                // The context is new or idle here, so there is no word to commit.
                let _ = self.engine.set_mode(id, mode);
            }
        }
    }

    // ------------------------------------------------------------------ keys

    pub fn would_consume(&self, ev: &KeyInput) -> bool {
        self.engine.would_consume(ev) || self.hotkey_would_consume(ev)
    }

    /// A chord hotkey (`Alt+Z`) swallows its key, so the host must route that key to us.
    fn hotkey_would_consume(&self, ev: &KeyInput) -> bool {
        [
            &self.hotkeys.toggle,
            &self.hotkeys.cycle,
            &self.hotkeys.open,
        ]
        .into_iter()
        .flatten()
        .any(|m| m.would_fire(ev))
    }

    /// Should the host send this key to [`Runtime::handle_key`]? True when the key will be
    /// consumed, or when it is not consumed but still produces actions (for example Space commits
    /// the word being composed and then reaches the application). TSF `OnTestKeyDown` returns
    /// this; `OnKeyDown` then reports only `consumed` as "eaten".
    pub fn would_handle(&self, ev: &KeyInput) -> bool {
        self.engine_would_handle(ev) || self.hotkey_would_consume(ev)
    }

    /// Process a key press or release. Modifier presses and releases must be fed in too, so that
    /// modifier-only hotkeys such as `Ctrl+Shift` work.
    ///
    /// Hosts that call a "test" and a "do" entry point per key (TSF) use the three steps
    /// [`Runtime::feed_hotkeys`], [`Runtime::run_hotkeys`] and [`Runtime::process_engine`]
    /// instead, so that every physical key event reaches the hotkey matchers exactly once.
    pub fn handle_key(&mut self, ev: KeyInput) -> KeyOutcome {
        let fired = self.feed_hotkeys(&ev);
        if fired.any() {
            self.run_hotkeys(ev.context_id, fired)
        } else {
            KeyOutcome {
                output: self.engine.process(ev),
                hotkey: None,
            }
        }
    }

    /// Step 1: show the event to the hotkey matchers (only their state changes).
    pub fn feed_hotkeys(&mut self, ev: &KeyInput) -> HotkeysFired {
        let fired = |m: &mut Option<HotkeyMatcher>| match m {
            Some(m) => (m.feed(ev), m.hotkey().key.is_some()),
            None => (false, false),
        };
        let (toggle, toggle_key) = fired(&mut self.hotkeys.toggle);
        let (cycle, cycle_key) = fired(&mut self.hotkeys.cycle);
        let (open, open_key) = fired(&mut self.hotkeys.open);
        HotkeysFired {
            toggle,
            cycle,
            open,
            swallow: (toggle && toggle_key) || (cycle && cycle_key) || (open && open_key),
        }
    }

    /// Step 2 (only when `fired.any()`): carry out the hotkeys. The mode toggle is applied here
    /// (a word in progress is committed); the others are reported in `KeyOutcome::hotkey`.
    pub fn run_hotkeys(&mut self, id: ContextId, fired: HotkeysFired) -> KeyOutcome {
        let mut outcome = KeyOutcome::default();
        if fired.toggle {
            let next = match self.engine.mode(id) {
                InputMode::Vietnamese => InputMode::English,
                InputMode::English => InputMode::Vietnamese,
            };
            outcome.output = self.engine.set_mode(id, next);
            self.remember_mode(id, next);
        }
        if fired.cycle {
            outcome.hotkey = Some(HotkeyAction::CycleScheme);
        } else if fired.open {
            outcome.hotkey = Some(HotkeyAction::OpenSettings);
        }
        outcome.output.consumed = fired.swallow;
        outcome
    }

    /// Step 3 (when no hotkey fired): the engine alone.
    pub fn process_engine(&mut self, ev: KeyInput) -> Output {
        self.engine.process(ev)
    }

    /// Would the engine (not the hotkeys) consume or react to this key? See [`Runtime::would_handle`].
    pub fn engine_would_handle(&self, ev: &KeyInput) -> bool {
        let out = self.engine.preview(ev);
        out.consumed || !out.actions.is_empty()
    }

    pub fn notify(&mut self, id: ContextId, n: Notify) -> Output {
        self.engine.notify(id, n)
    }

    /// The user (or the host UI) switched this context. Remembered according to `mode_scope`.
    pub fn set_mode(&mut self, id: ContextId, mode: InputMode) -> Output {
        self.remember_mode(id, mode);
        self.engine.set_mode(id, mode)
    }

    fn remember_mode(&mut self, id: ContextId, mode: InputMode) {
        match self.config.general.mode_scope {
            ime_config::ModeScope::Global => self.global_mode = mode,
            ime_config::ModeScope::PerApp => {
                if let Some(key) = self.apps.get(&id).and_then(AppIdentity::app_key) {
                    self.app_modes.insert(key.to_string(), mode);
                }
            }
        }
    }

    /// The mode shared by all applications. Hosts that keep it in a system-wide place (a TSF
    /// compartment) publish it after a toggle and feed it back here on focus changes.
    pub fn global_mode(&self) -> InputMode {
        self.global_mode
    }

    pub fn set_global_mode(&mut self, mode: InputMode) {
        self.global_mode = mode;
    }

    /// Bring a context in line with the shared / remembered mode. Call it when the context gets
    /// focus. Contexts whose profile fixes the mode (an override, game mode) are left alone.
    pub fn sync_mode(&mut self, id: ContextId) -> Output {
        let Some(app) = self.apps.get(&id) else {
            return Output::default();
        };
        let eff = self.config.resolve(app);
        let fixed = eff.game_mode
            || self
                .config
                .matching_profile(app)
                .is_some_and(|p| p.overrides.mode.is_some());
        if fixed {
            return Output::default();
        }
        let want = match self.config.general.mode_scope {
            ime_config::ModeScope::Global => self.global_mode,
            ime_config::ModeScope::PerApp => app
                .app_key()
                .and_then(|k| self.app_modes.get(k).copied())
                .unwrap_or(self.config.general.default_mode),
        };
        if self.engine.mode(id) == want {
            Output::default()
        } else {
            self.engine.set_mode(id, want)
        }
    }

    pub fn mode(&self, id: ContextId) -> InputMode {
        self.engine.mode(id)
    }

    /// Change the configuration (a tray menu, a settings window), apply it at once and, when the
    /// runtime was loaded from files, save it. Returns the save error if there was one; the new
    /// settings stay applied for this session either way.
    pub fn update_config(&mut self, change: impl FnOnce(&mut Config)) -> std::io::Result<()> {
        let mut config = (*self.config).clone();
        change(&mut config);
        let macros = self.macro_list.clone();
        self.apply(config, macros);
        let Some(paths) = self.paths.clone() else {
            return Ok(());
        };
        let saved = ime_config::save_atomic(&paths.config_file, &self.config);
        // Our own write must not look like an outside edit.
        self.stamp.config = mtime(&paths.config_file);
        saved
    }

    /// The macros currently in use.
    pub fn macros(&self) -> &[ime_core::Macro] {
        &self.macro_list
    }

    /// Replace all macros: validate, apply at once and, when loaded from files, save
    /// `macros.json`. Nothing changes if the list is invalid.
    pub fn update_macros(&mut self, list: Vec<ime_core::Macro>) -> Result<(), String> {
        let max = self.config.macros.max_expansion_chars as usize;
        let issues = ime_macro::validate(&list, max);
        if !issues.is_empty() {
            return Err(format!("{} mục gõ tắt không hợp lệ", issues.len()));
        }
        let config = (*self.config).clone();
        self.apply(config, list);
        let Some(paths) = self.paths.clone() else {
            return Ok(());
        };
        ime_macro::store::save_atomic(&paths.macros_file, &self.macro_list)
            .map_err(|e| e.to_string())?;
        self.stamp.macros = mtime(&paths.macros_file);
        Ok(())
    }

    /// Switch to the next typing scheme (Telex -> VNI -> VIQR -> Telex) for this session. The
    /// change lives in memory only and is replaced the next time the files are reloaded.
    pub fn cycle_scheme(&mut self) -> ime_core::Scheme {
        use ime_core::Scheme;
        let mut config = (*self.config).clone();
        config.general.scheme = match config.general.scheme {
            Scheme::Telex => Scheme::Vni,
            Scheme::Vni => Scheme::Viqr,
            Scheme::Viqr => Scheme::Telex,
        };
        let scheme = config.general.scheme;
        let macros = self.macro_list.clone();
        self.apply(config, macros);
        scheme
    }

    /// Forget modifier state, e.g. when focus moves and key-up events may have been missed.
    pub fn reset_hotkeys(&mut self) {
        for m in [
            &mut self.hotkeys.toggle,
            &mut self.hotkeys.cycle,
            &mut self.hotkeys.open,
        ]
        .into_iter()
        .flatten()
        {
            m.reset();
        }
    }
}

fn has_mode_override(config: &Config, app: &AppIdentity) -> bool {
    config
        .matching_profile(app)
        .is_some_and(|p| p.overrides.mode.is_some())
}

fn load_files(paths: &Paths) -> (Config, Vec<ime_core::Macro>, Option<String>, Stamp) {
    let rec = ime_config::load_or_recover(&paths.config_file);
    let mut error = rec.error;
    let max = rec.config.macros.max_expansion_chars as usize;
    let macros = match ime_macro::store::load(&paths.macros_file, max) {
        Ok(m) => m,
        Err(e) => {
            error.get_or_insert_with(|| e.to_string());
            Vec::new()
        }
    };
    let stamp = Stamp {
        config: mtime(&paths.config_file),
        macros: mtime(&paths.macros_file),
    };
    (rec.config, macros, error, stamp)
}

/// True when the output asks the host to end or change the composition.
pub fn touches_composition(out: &Output) -> bool {
    out.actions
        .iter()
        .any(|a| !matches!(a, EngineAction::ModeChanged(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ime_config::{MatchKind, MatchPattern, Profile, ProfileMatch, ProfileOverride};
    use ime_core::{KeyFlags, LogicalKey, Scheme};

    fn key(id: ContextId, ch: char) -> KeyInput {
        KeyInput::character(id, ch)
    }

    fn type_str(rt: &mut Runtime, id: ContextId, s: &str) -> Vec<KeyOutcome> {
        s.chars().map(|c| rt.handle_key(key(id, c))).collect()
    }

    fn commits(outs: &[KeyOutcome]) -> Vec<String> {
        outs.iter()
            .flat_map(|o| &o.output.actions)
            .filter_map(|a| match a {
                EngineAction::Commit(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    fn modifier(id: ContextId, code: u32, down: bool) -> KeyInput {
        let mut k = KeyInput::new(id, LogicalKey::Other);
        k.physical_code = code;
        if !down {
            k.flags |= KeyFlags::KEY_UP;
        }
        k
    }

    fn code_profile(exe: &str, f: impl FnOnce(&mut ProfileOverride)) -> Profile {
        let mut o = ProfileOverride::default();
        f(&mut o);
        Profile {
            id: "123e4567-e89b-12d3-a456-426614174000".into(),
            name: "p".into(),
            enabled: true,
            matcher: ProfileMatch {
                kind: MatchKind::ExeName,
                value: exe.into(),
                pattern: MatchPattern::Exact,
            },
            overrides: o,
        }
    }

    fn app(exe: &str) -> AppIdentity {
        AppIdentity {
            exe_name: Some(exe.into()),
            ..Default::default()
        }
    }

    #[test]
    fn types_vietnamese_with_default_settings() {
        let mut rt = Runtime::new(Config::default(), Vec::new());
        let id = rt.create_context(AppIdentity::default(), Capabilities::PREEDIT);
        let outs = type_str(&mut rt, id, "tieengs ");
        assert_eq!(commits(&outs), ["tiếng"]);
    }

    #[test]
    fn profile_overrides_apply_per_application() {
        let mut cfg = Config::default();
        cfg.profiles.push(code_profile("code.exe", |o| {
            o.scheme = Some(Scheme::Vni);
        }));
        let mut rt = Runtime::new(cfg, Vec::new());
        let vs = rt.create_context(app("code.exe"), Capabilities::empty());
        let notepad = rt.create_context(app("notepad.exe"), Capabilities::empty());
        assert_eq!(commits(&type_str(&mut rt, vs, "tie6ng ")), ["tiêng"]);
        assert_eq!(commits(&type_str(&mut rt, notepad, "tieeng ")), ["tiêng"]);
    }

    #[test]
    fn game_mode_and_english_profiles_start_in_english() {
        let mut cfg = Config::default();
        cfg.profiles
            .push(code_profile("game.exe", |o| o.game_mode = Some(true)));
        let mut rt = Runtime::new(cfg, Vec::new());
        let g = rt.create_context(app("game.exe"), Capabilities::empty());
        assert_eq!(rt.mode(g), InputMode::English);
        assert!(type_str(&mut rt, g, "tieengs ")
            .iter()
            .all(|o| !o.output.consumed));
    }

    #[test]
    fn macros_expand_and_respect_profile_switch() {
        let mut cfg = Config::default();
        cfg.profiles
            .push(code_profile("code.exe", |o| o.macros_enabled = Some(false)));
        let mut m = ime_core::Macro::new("vn", "Việt Nam");
        m.id = "1".into();
        let mut rt = Runtime::new(cfg, vec![m]);
        let a = rt.create_context(AppIdentity::default(), Capabilities::empty());
        let b = rt.create_context(app("code.exe"), Capabilities::empty());
        assert_eq!(commits(&type_str(&mut rt, a, "vn ")), ["Việt Nam"]);
        assert_eq!(commits(&type_str(&mut rt, b, "vn ")), ["vn"]);
    }

    #[test]
    fn invalid_macros_disable_expansion_but_keep_typing() {
        let bad = ime_core::Macro::new("vn", "x"); // no id
        let mut rt = Runtime::new(Config::default(), vec![bad]);
        assert!(rt.last_error.is_some());
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        assert_eq!(commits(&type_str(&mut rt, id, "vn ")), ["vn"]);
    }

    #[test]
    fn modifier_only_hotkey_toggles_the_mode() {
        let mut rt = Runtime::new(Config::default(), Vec::new());
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        const CTRL: u32 = 0xE0;
        const SHIFT: u32 = 0xE1;
        assert_eq!(rt.mode(id), InputMode::Vietnamese);
        for (code, down) in [(CTRL, true), (SHIFT, true), (SHIFT, false), (CTRL, false)] {
            let _ = rt.handle_key(modifier(id, code, down));
        }
        assert_eq!(rt.mode(id), InputMode::English);
        // Typing now passes through untouched.
        assert!(type_str(&mut rt, id, "a")
            .iter()
            .all(|o| !o.output.consumed));
        for (code, down) in [(CTRL, true), (SHIFT, true), (CTRL, false), (SHIFT, false)] {
            let _ = rt.handle_key(modifier(id, code, down));
        }
        assert_eq!(rt.mode(id), InputMode::Vietnamese);
    }

    #[test]
    fn update_config_applies_and_saves() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::in_dirs(dir.path().to_path_buf(), dir.path().join("rt"));
        let mut rt = Runtime::from_paths(paths.clone());
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        rt.update_config(|c| c.general.scheme = Scheme::Vni)
            .unwrap();
        assert_eq!(commits(&type_str(&mut rt, id, "tie6ng ")), ["tiêng"]);
        let saved = ime_config::load_file(&paths.config_file).unwrap().config;
        assert_eq!(saved.general.scheme, Scheme::Vni);
        assert!(!rt.poll_reload(), "our own save is not an outside change");
    }

    #[test]
    fn update_macros_validates_applies_and_saves() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::in_dirs(dir.path().to_path_buf(), dir.path().join("rt"));
        let mut rt = Runtime::from_paths(paths.clone());
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        let mut m = ime_core::Macro::new("vn", "Việt Nam");
        m.id = ime_macro::store::new_id();
        rt.update_macros(vec![m.clone()]).unwrap();
        assert_eq!(commits(&type_str(&mut rt, id, "vn ")), ["Việt Nam"]);
        assert_eq!(
            ime_macro::store::load(&paths.macros_file, 4096).unwrap(),
            [m.clone()]
        );
        // An invalid list (no id) is refused and changes nothing.
        let bad = ime_core::Macro::new("x", "y");
        assert!(rt.update_macros(vec![bad]).is_err());
        assert_eq!(rt.macros().len(), 1);
        assert!(!rt.poll_reload(), "our own save is not an outside change");
    }

    #[test]
    fn cycle_scheme_visits_every_scheme_and_applies_it() {
        let mut rt = Runtime::new(Config::default(), Vec::new());
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        assert_eq!(rt.cycle_scheme(), Scheme::Vni);
        assert_eq!(commits(&type_str(&mut rt, id, "tie6ng ")), ["tiêng"]);
        assert_eq!(rt.cycle_scheme(), Scheme::Viqr);
        assert_eq!(commits(&type_str(&mut rt, id, "tie^ng ")), ["tiêng"]);
        assert_eq!(rt.cycle_scheme(), Scheme::Telex);
        assert_eq!(commits(&type_str(&mut rt, id, "tieeng ")), ["tiêng"]);
    }

    fn toggle(rt: &mut Runtime, id: ContextId) {
        for (code, down) in [(0xE0, true), (0xE1, true), (0xE1, false), (0xE0, false)] {
            let _ = rt.handle_key(modifier(id, code, down));
        }
    }

    #[test]
    fn global_mode_follows_the_user_across_applications() {
        let mut rt = Runtime::new(Config::default(), Vec::new());
        let a = rt.create_context(app("a.exe"), Capabilities::empty());
        let b = rt.create_context(app("b.exe"), Capabilities::empty());
        toggle(&mut rt, a);
        assert_eq!(rt.mode(a), InputMode::English);
        assert_eq!(rt.global_mode(), InputMode::English);
        // The other application catches up when it gets focus.
        assert_eq!(rt.mode(b), InputMode::Vietnamese);
        rt.sync_mode(b);
        assert_eq!(rt.mode(b), InputMode::English);
        // A new application starts in the shared mode.
        let c = rt.create_context(app("c.exe"), Capabilities::empty());
        assert_eq!(rt.mode(c), InputMode::English);
        toggle(&mut rt, c);
        rt.sync_mode(a);
        assert_eq!(rt.mode(a), InputMode::Vietnamese);
    }

    #[test]
    fn a_host_can_publish_the_shared_mode() {
        let mut rt = Runtime::new(Config::default(), Vec::new());
        let a = rt.create_context(app("a.exe"), Capabilities::empty());
        // Another process toggled; this host read it from the system-wide store.
        rt.set_global_mode(InputMode::English);
        rt.sync_mode(a);
        assert_eq!(rt.mode(a), InputMode::English);
    }

    #[test]
    fn per_app_mode_is_remembered_per_application() {
        let mut cfg = Config::default();
        cfg.general.mode_scope = ime_config::ModeScope::PerApp;
        let mut rt = Runtime::new(cfg, Vec::new());
        let a = rt.create_context(app("a.exe"), Capabilities::empty());
        let b = rt.create_context(app("b.exe"), Capabilities::empty());
        toggle(&mut rt, a);
        rt.sync_mode(b);
        assert_eq!(rt.mode(a), InputMode::English);
        assert_eq!(rt.mode(b), InputMode::Vietnamese, "b is unaffected");
        // A second window of a.exe starts the way a.exe was left.
        let a2 = rt.create_context(app("a.exe"), Capabilities::empty());
        assert_eq!(rt.mode(a2), InputMode::English);
    }

    #[test]
    fn profile_fixed_modes_are_not_overridden_by_the_shared_mode() {
        let mut cfg = Config::default();
        cfg.profiles.push(code_profile("code.exe", |o| {
            o.mode = Some(InputMode::English)
        }));
        cfg.profiles[0].id = "123e4567-e89b-12d3-a456-426614174001".into();
        let mut rt = Runtime::new(cfg, Vec::new());
        let ide = rt.create_context(app("code.exe"), Capabilities::empty());
        assert_eq!(rt.mode(ide), InputMode::English);
        rt.sync_mode(ide);
        assert_eq!(
            rt.mode(ide),
            InputMode::English,
            "global is Vietnamese but the profile wins"
        );
        let other = rt.create_context(app("x.exe"), Capabilities::empty());
        toggle(&mut rt, other);
        rt.sync_mode(ide);
        assert_eq!(rt.mode(ide), InputMode::English);
    }

    #[test]
    fn would_handle_includes_keys_that_only_commit() {
        let mut rt = Runtime::new(Config::default(), Vec::new());
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        let space = key(id, ' ');
        // Nothing to commit: the key is the application's.
        assert!(!rt.would_handle(&space));
        type_str(&mut rt, id, "xin");
        assert!(!rt.would_consume(&space), "space itself is not consumed");
        assert!(rt.would_handle(&space), "but it commits the word first");
        // Asking never changes state.
        let outs = type_str(&mut rt, id, " ");
        assert_eq!(commits(&outs), ["xin"]);
    }

    #[test]
    fn would_handle_sees_chord_hotkeys_without_firing_them() {
        let mut cfg = Config::default();
        cfg.hotkeys.toggle_mode = Some("Alt+Z".parse().unwrap());
        let mut rt = Runtime::new(cfg, Vec::new());
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        let mut z = KeyInput::new(id, LogicalKey::Other);
        z.physical_code = 0x1D;
        assert!(!rt.would_handle(&z), "Alt is not held yet");
        rt.handle_key(modifier(id, 0xE2, true));
        assert!(rt.would_handle(&z));
        assert!(rt.would_handle(&z), "still true: the query changed nothing");
        assert_eq!(rt.mode(id), InputMode::Vietnamese);
        assert!(rt.handle_key(z).output.consumed);
        assert_eq!(rt.mode(id), InputMode::English);
    }

    #[test]
    fn toggling_commits_the_word_in_progress() {
        let mut rt = Runtime::new(Config::default(), Vec::new());
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        type_str(&mut rt, id, "caay");
        let mut outs = Vec::new();
        for (code, down) in [(0xE0, true), (0xE1, true), (0xE1, false), (0xE0, false)] {
            outs.push(rt.handle_key(modifier(id, code, down)));
        }
        assert_eq!(commits(&outs), ["cây"]);
    }

    #[test]
    fn chord_hotkey_consumes_its_key_and_other_hotkeys_are_reported() {
        let mut cfg = Config::default();
        cfg.hotkeys.toggle_mode = Some("Alt+Z".parse().unwrap());
        cfg.hotkeys.cycle_scheme = Some("Ctrl+Alt+S".parse().unwrap());
        let mut rt = Runtime::new(cfg, Vec::new());
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        let (alt, ctrl, z, s) = (0xE2, 0xE0, 0x1D, 0x16);
        rt.handle_key(modifier(id, alt, true));
        let o = rt.handle_key(modifier(id, z, true));
        assert!(o.output.consumed);
        assert_eq!(rt.mode(id), InputMode::English);
        rt.handle_key(modifier(id, z, false));
        rt.handle_key(modifier(id, ctrl, true));
        let o = rt.handle_key(modifier(id, s, true));
        assert_eq!(o.hotkey, Some(HotkeyAction::CycleScheme));
        assert!(o.output.consumed);
    }

    #[test]
    fn files_are_loaded_and_reloaded_when_they_change() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::in_dirs(dir.path().to_path_buf(), dir.path().join("rt"));
        let mut rt = Runtime::from_paths(paths.clone());
        assert!(rt.last_error.is_none());
        assert!(!rt.poll_reload(), "nothing changed yet");
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        assert_eq!(commits(&type_str(&mut rt, id, "tie6ng ")), ["tie", "ng"]);

        // Switch to VNI by editing the file.
        std::fs::write(&paths.config_file, "[general]\nscheme = \"vni\"\n").unwrap();
        // File times can have coarse resolution; make sure the stamp differs.
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&paths.config_file)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(rt.poll_reload());
        assert_eq!(commits(&type_str(&mut rt, id, "tie6ng ")), ["tiêng"]);

        // A broken file keeps the running settings and reports the problem.
        std::fs::write(&paths.config_file, "[general\n").unwrap();
        let later = SystemTime::now() + std::time::Duration::from_secs(10);
        std::fs::File::options()
            .write(true)
            .open(&paths.config_file)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(!rt.poll_reload());
        assert!(rt.last_error.is_some());
        assert_eq!(commits(&type_str(&mut rt, id, "tie6ng ")), ["tiêng"]);
    }

    #[test]
    fn macros_json_is_picked_up() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::in_dirs(dir.path().to_path_buf(), dir.path().join("rt"));
        let mut m = ime_core::Macro::new("vn", "Việt Nam");
        m.id = ime_macro::store::new_id();
        ime_macro::store::save_atomic(&paths.macros_file, &[m]).unwrap();
        let mut rt = Runtime::from_paths(paths);
        let id = rt.create_context(AppIdentity::default(), Capabilities::empty());
        assert_eq!(commits(&type_str(&mut rt, id, "vn ")), ["Việt Nam"]);
    }
}
