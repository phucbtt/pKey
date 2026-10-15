use std::collections::HashMap;
use std::sync::Arc;

use ime_macro::MacroTable;
use ime_unicode::{caret_at_end, to_nfd};

use crate::composition::{accepts, Composition};
use crate::config::{EngineConfig, RestoreKey};
use crate::types::{
    Capabilities, ContextId, ContextInfo, EngineAction, InputMode, KeyFlags, KeyInput, LogicalKey,
    MacroDelimiters, Modifiers, Notify, Output, OutputEncoding,
};

/// Longest word kept in one composition (spec §7.2, I3). A longer run is committed and restarted.
pub const MAX_COMPOSITION: usize = 32;

#[derive(Clone, Debug)]
struct Context {
    mode: InputMode,
    caps: Capabilities,
    comp: Composition,
    /// Macros are switched off for this context (profile override, game mode).
    macros_off: bool,
    /// Active profile id, used to pick profile-scoped macros.
    profile: Option<Arc<str>>,
    /// Typing options after the profile's overrides; `None` uses the engine-wide ones.
    cfg: Option<Arc<EngineConfig>>,
}

/// Macro table plus the delimiters that trigger an expansion.
#[derive(Clone, Debug, Default)]
struct MacroState {
    table: Option<Arc<MacroTable>>,
    delimiters: MacroDelimiters,
}

impl Context {
    fn new(cfg: &EngineConfig, caps: Capabilities) -> Self {
        Context {
            mode: cfg.default_mode,
            caps,
            comp: Composition::default(),
            macros_off: false,
            profile: None,
            cfg: None,
        }
    }
}

/// The Vietnamese input engine. One instance serves many input contexts; each context has its
/// own composition buffer and mode, and contexts never influence each other.
#[derive(Debug)]
pub struct Engine {
    cfg: Arc<EngineConfig>,
    macros: MacroState,
    contexts: HashMap<ContextId, Context>,
    next_id: ContextId,
}

impl Engine {
    pub fn new(cfg: Arc<EngineConfig>) -> Self {
        Engine {
            cfg,
            macros: MacroState::default(),
            contexts: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn config(&self) -> &EngineConfig {
        &self.cfg
    }

    /// Swap the configuration. Words already being composed keep their letters; the new options
    /// apply from the next key.
    pub fn reload_config(&mut self, cfg: Arc<EngineConfig>) {
        self.cfg = cfg;
    }

    /// Install (or remove, with `None`) the macro table. `delimiters` chooses which keys that end
    /// a word also expand it.
    pub fn set_macros(&mut self, table: Option<Arc<MacroTable>>, delimiters: MacroDelimiters) {
        self.macros = MacroState { table, delimiters };
    }

    /// Per-context macro settings from the matching profile: `enabled` is false for game mode or
    /// a profile with `macros_enabled = false`; `profile` selects profile-scoped macros.
    pub fn set_context_macros(&mut self, id: ContextId, enabled: bool, profile: Option<&str>) {
        let cfg = Arc::clone(&self.cfg);
        let ctx = self
            .contexts
            .entry(id)
            .or_insert_with(|| Context::new(&cfg, Capabilities::empty()));
        ctx.macros_off = !enabled;
        ctx.profile = profile.map(Arc::from);
    }

    /// Typing options for one context (the matching profile's overrides). `None` goes back to the
    /// engine-wide options. A word in progress keeps its letters; the new options apply from the
    /// next key.
    pub fn set_context_config(&mut self, id: ContextId, cfg: Option<Arc<EngineConfig>>) {
        let global = Arc::clone(&self.cfg);
        let ctx = self
            .contexts
            .entry(id)
            .or_insert_with(|| Context::new(&global, Capabilities::empty()));
        ctx.cfg = cfg;
    }

    pub fn create_context(&mut self, info: ContextInfo) -> ContextId {
        let id = self.next_id;
        self.next_id += 1;
        self.contexts.insert(id, Context::new(&self.cfg, info.caps));
        id
    }

    pub fn destroy_context(&mut self, id: ContextId) {
        self.contexts.remove(&id);
    }

    pub fn mode(&self, id: ContextId) -> InputMode {
        self.contexts
            .get(&id)
            .map_or(self.cfg.default_mode, |c| c.mode)
    }

    pub fn has_context(&self, id: ContextId) -> bool {
        self.contexts.contains_key(&id)
    }

    /// What [`Engine::process`] would return for `ev`, without changing any state. A TSF text
    /// service needs this in `OnTestKeyDown`: a key that is not consumed can still have actions
    /// (Space commits the word, then reaches the application).
    pub fn preview(&self, ev: &KeyInput) -> Output {
        let mut ctx = self
            .contexts
            .get(&ev.context_id)
            .cloned()
            .unwrap_or_else(|| Context::new(&self.cfg, Capabilities::empty()));
        let cfg = ctx.cfg.clone().unwrap_or_else(|| Arc::clone(&self.cfg));
        step(&cfg, &self.macros, &mut ctx, ev)
    }

    /// Same decision as [`Engine::process`] without changing any state.
    pub fn would_consume(&self, ev: &KeyInput) -> bool {
        self.preview(ev).consumed
    }

    pub fn process(&mut self, ev: KeyInput) -> Output {
        let cfg = Arc::clone(&self.cfg);
        let ctx = self
            .contexts
            .entry(ev.context_id)
            .or_insert_with(|| Context::new(&cfg, Capabilities::empty()));
        let cfg = ctx.cfg.clone().unwrap_or(cfg);
        step(&cfg, &self.macros, ctx, &ev)
    }

    pub fn notify(&mut self, id: ContextId, n: Notify) -> Output {
        let cfg = Arc::clone(&self.cfg);
        let ctx = self
            .contexts
            .entry(id)
            .or_insert_with(|| Context::new(&cfg, Capabilities::empty()));
        let cfg = ctx.cfg.clone().unwrap_or(cfg);
        let mut out = Output::default();
        match n {
            Notify::FocusIn { caps } => {
                if !ctx.comp.is_empty() {
                    flush(&cfg, ctx, &mut out);
                }
                ctx.caps = caps;
            }
            Notify::FocusOut | Notify::CaretMoved | Notify::SelectionChanged | Notify::Reset => {
                if !ctx.comp.is_empty() {
                    flush(&cfg, ctx, &mut out);
                }
            }
            Notify::CompositionTerminatedByHost | Notify::HostCommitFailed => {
                ctx.comp.clear();
            }
        }
        out
    }

    /// Switch Vietnamese/English for one context. A word in progress is committed first.
    pub fn set_mode(&mut self, id: ContextId, mode: InputMode) -> Output {
        let cfg = Arc::clone(&self.cfg);
        let ctx = self
            .contexts
            .entry(id)
            .or_insert_with(|| Context::new(&cfg, Capabilities::empty()));
        let cfg = ctx.cfg.clone().unwrap_or(cfg);
        let mut out = Output::default();
        if !ctx.comp.is_empty() {
            commit(&cfg, ctx, &mut out);
        }
        if ctx.mode != mode {
            ctx.mode = mode;
            out.actions.push(EngineAction::ModeChanged(mode));
        }
        out
    }
}

fn commit(cfg: &EngineConfig, ctx: &mut Context, out: &mut Output) {
    let text = ctx.comp.finish(cfg);
    ctx.comp.clear();
    out.actions.push(EngineAction::Commit(text));
}

/// Commit at a word delimiter: a matching macro replaces the word. Secure input never gets here
/// (the engine passes everything through before reaching a commit).
fn commit_at(
    cfg: &EngineConfig,
    mac: &MacroState,
    ctx: &mut Context,
    out: &mut Output,
    delimiter: MacroDelimiters,
) {
    // `contains(empty)` is always true, so a non-delimiter (a digit) must be excluded explicitly.
    if !ctx.macros_off && !delimiter.is_empty() && mac.delimiters.contains(delimiter) {
        if let Some(table) = &mac.table {
            let raw = ctx.comp.raw_string();
            let shown = ctx.comp.shown_nfc(cfg);
            if let Some(mut text) = table.expand(&raw, &shown, ctx.profile.as_deref()) {
                if cfg.encoding == OutputEncoding::UnicodeNfd {
                    text = to_nfd(&text);
                }
                ctx.comp.clear();
                out.actions.push(EngineAction::Commit(text));
                return;
            }
        }
    }
    commit(cfg, ctx, out);
}

/// Focus loss and similar: commit or drop the word depending on the setting.
fn flush(cfg: &EngineConfig, ctx: &mut Context, out: &mut Output) {
    if cfg.commit_on_focus_loss {
        commit(cfg, ctx, out);
    } else {
        ctx.comp.clear();
        out.actions.push(EngineAction::CancelComposition);
    }
}

fn preedit(cfg: &EngineConfig, ctx: &Context, out: &mut Output) {
    let text = ctx.comp.render(cfg);
    let caret = caret_at_end(&text);
    out.actions
        .push(EngineAction::UpdatePreedit { text, caret });
}

fn step(cfg: &EngineConfig, mac: &MacroState, ctx: &mut Context, ev: &KeyInput) -> Output {
    let mut out = Output::default();
    if ev.flags.intersects(KeyFlags::KEY_UP | KeyFlags::SYNTHETIC) {
        return out;
    }
    if ctx.mode == InputMode::English || ctx.caps.contains(Capabilities::SECURE_INPUT) {
        if !ctx.comp.is_empty() {
            commit(cfg, ctx, &mut out);
        }
        return out;
    }

    let mut mods = ev.modifiers;
    if mods.contains(Modifiers::ALT_GR) {
        mods.remove(Modifiers::CTRL | Modifiers::ALT);
    }
    if mods.intersects(Modifiers::CTRL | Modifiers::ALT | Modifiers::META) {
        if !ctx.comp.is_empty() {
            commit(cfg, ctx, &mut out);
        }
        return out;
    }

    let composing = !ctx.comp.is_empty();
    match ev.logical_key {
        LogicalKey::Char => {
            let Some(ch) = ev.text else {
                if composing {
                    commit(cfg, ctx, &mut out);
                }
                return out;
            };
            if composing && ctx.comp.len() >= MAX_COMPOSITION {
                commit(cfg, ctx, &mut out);
            }
            if accepts(ch, cfg, !ctx.comp.is_empty()) {
                ctx.comp.push_key(ch, cfg);
                out.consumed = true;
                preedit(cfg, ctx, &mut out);
            } else if !ctx.comp.is_empty() {
                let delimiter = if ch.is_alphanumeric() {
                    MacroDelimiters::empty()
                } else {
                    MacroDelimiters::PUNCTUATION
                };
                commit_at(cfg, mac, ctx, &mut out, delimiter);
            }
        }
        LogicalKey::Backspace => {
            if composing {
                out.consumed = true;
                if ctx.comp.backspace(cfg) {
                    preedit(cfg, ctx, &mut out);
                } else {
                    ctx.comp.clear();
                    out.actions.push(EngineAction::CancelComposition);
                }
            }
        }
        LogicalKey::Escape => {
            if composing {
                match cfg.restore_key {
                    RestoreKey::Esc => {
                        let raw = ctx.comp.raw_string();
                        ctx.comp.clear();
                        out.actions.push(EngineAction::Commit(raw));
                        out.consumed = true;
                    }
                    RestoreKey::None => commit(cfg, ctx, &mut out),
                }
            }
        }
        LogicalKey::Space | LogicalKey::Enter | LogicalKey::Tab => {
            if composing {
                let delimiter = match ev.logical_key {
                    LogicalKey::Space => MacroDelimiters::SPACE,
                    LogicalKey::Enter => MacroDelimiters::ENTER,
                    _ => MacroDelimiters::TAB,
                };
                commit_at(cfg, mac, ctx, &mut out, delimiter);
            }
        }
        LogicalKey::Delete
        | LogicalKey::Left
        | LogicalKey::Right
        | LogicalKey::Up
        | LogicalKey::Down
        | LogicalKey::Home
        | LogicalKey::End
        | LogicalKey::PageUp
        | LogicalKey::PageDown
        | LogicalKey::Other => {
            if composing {
                commit(cfg, ctx, &mut out);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Scheme;
    use ime_macro::Macro;

    fn engine() -> (Engine, ContextId) {
        let mut e = Engine::new(Arc::new(EngineConfig::default()));
        let id = e.create_context(ContextInfo::default());
        (e, id)
    }

    fn type_str(e: &mut Engine, id: ContextId, s: &str) -> Vec<Output> {
        s.chars()
            .map(|c| e.process(KeyInput::character(id, c)))
            .collect()
    }

    #[test]
    fn letter_is_consumed_and_shows_preedit() {
        let (mut e, id) = engine();
        let out = e.process(KeyInput::character(id, 'a'));
        assert!(out.consumed);
        assert!(matches!(
            out.actions.as_slice(),
            [EngineAction::UpdatePreedit { text, .. }] if text == "a"
        ));
        let out = e.process(KeyInput::character(id, 's'));
        assert!(matches!(
            out.actions.as_slice(),
            [EngineAction::UpdatePreedit { text, .. }] if text == "á"
        ));
    }

    #[test]
    fn space_commits_and_passes_through() {
        let (mut e, id) = engine();
        type_str(&mut e, id, "tieengs");
        let out = e.process(KeyInput::character(id, ' '));
        assert!(!out.consumed);
        assert_eq!(out.actions, vec![EngineAction::Commit("tiếng".into())]);
    }

    #[test]
    fn idle_non_letters_pass_through_untouched() {
        let (mut e, id) = engine();
        for c in ['1', '.', ',', ' ', '-'] {
            let out = e.process(KeyInput::character(id, c));
            assert_eq!(out, Output::default(), "{c}");
        }
        let out = e.process(KeyInput::new(id, LogicalKey::Backspace));
        assert_eq!(out, Output::default());
    }

    #[test]
    fn would_consume_matches_process_and_keeps_state() {
        let (mut e, id) = engine();
        for c in "tieengs vieejt".chars() {
            let ev = KeyInput::character(id, c);
            let predicted = e.would_consume(&ev);
            let out = e.process(ev);
            assert_eq!(predicted, out.consumed, "{c}");
        }
    }

    #[test]
    fn english_mode_and_secure_input_pass_through() {
        let (mut e, id) = engine();
        let out = e.set_mode(id, InputMode::English);
        assert_eq!(
            out.actions,
            vec![EngineAction::ModeChanged(InputMode::English)]
        );
        for o in type_str(&mut e, id, "tieengs") {
            assert_eq!(o, Output::default());
        }
        e.set_mode(id, InputMode::Vietnamese);
        e.notify(
            id,
            Notify::FocusIn {
                caps: Capabilities::SECURE_INPUT,
            },
        );
        for o in type_str(&mut e, id, "tieengs") {
            assert_eq!(o, Output::default());
        }
    }

    #[test]
    fn set_mode_commits_word_in_progress() {
        let (mut e, id) = engine();
        type_str(&mut e, id, "caay");
        let out = e.set_mode(id, InputMode::English);
        assert_eq!(
            out.actions,
            vec![
                EngineAction::Commit("cây".into()),
                EngineAction::ModeChanged(InputMode::English)
            ]
        );
    }

    #[test]
    fn modifiers_commit_and_pass_through() {
        let (mut e, id) = engine();
        type_str(&mut e, id, "caay");
        let mut ev = KeyInput::character(id, 'c');
        ev.modifiers = Modifiers::CTRL;
        let out = e.process(ev);
        assert!(!out.consumed);
        assert_eq!(out.actions, vec![EngineAction::Commit("cây".into())]);
    }

    #[test]
    fn synthetic_and_key_up_are_ignored() {
        let (mut e, id) = engine();
        let mut ev = KeyInput::character(id, 'a');
        ev.flags = KeyFlags::SYNTHETIC;
        assert_eq!(e.process(ev), Output::default());
        ev.flags = KeyFlags::KEY_UP;
        assert_eq!(e.process(ev), Output::default());
    }

    #[test]
    fn contexts_are_isolated() {
        let mut e = Engine::new(Arc::new(EngineConfig::default()));
        let a = e.create_context(ContextInfo::default());
        let b = e.create_context(ContextInfo::default());
        type_str(&mut e, a, "tieeng");
        type_str(&mut e, b, "xin");
        let out = e.notify(a, Notify::FocusOut);
        assert_eq!(out.actions, vec![EngineAction::Commit("tiêng".into())]);
        let out = e.process(KeyInput::character(b, ' '));
        assert_eq!(out.actions, vec![EngineAction::Commit("xin".into())]);
    }

    #[test]
    fn host_termination_drops_state_silently() {
        let (mut e, id) = engine();
        type_str(&mut e, id, "tieeng");
        assert_eq!(
            e.notify(id, Notify::CompositionTerminatedByHost),
            Output::default()
        );
        let out = e.process(KeyInput::character(id, ' '));
        assert_eq!(out, Output::default());
    }

    #[test]
    fn focus_loss_can_cancel_instead_of_commit() {
        let cfg = EngineConfig {
            commit_on_focus_loss: false,
            ..EngineConfig::default()
        };
        let mut e = Engine::new(Arc::new(cfg));
        let id = e.create_context(ContextInfo::default());
        type_str(&mut e, id, "tieeng");
        let out = e.notify(id, Notify::FocusOut);
        assert_eq!(out.actions, vec![EngineAction::CancelComposition]);
    }

    #[test]
    fn long_runs_are_split_at_max_composition() {
        let (mut e, id) = engine();
        let word = "b".repeat(MAX_COMPOSITION + 3);
        let outs = type_str(&mut e, id, &word);
        let commits: usize = outs
            .iter()
            .flat_map(|o| &o.actions)
            .filter(|a| matches!(a, EngineAction::Commit(_)))
            .count();
        assert_eq!(commits, 1);
    }

    #[test]
    fn vni_digits_only_inside_a_word() {
        let cfg = EngineConfig {
            scheme: Scheme::Vni,
            ..EngineConfig::default()
        };
        let mut e = Engine::new(Arc::new(cfg));
        let id = e.create_context(ContextInfo::default());
        assert_eq!(e.process(KeyInput::character(id, '1')), Output::default());
        type_str(&mut e, id, "a1");
        let out = e.process(KeyInput::character(id, ' '));
        assert_eq!(out.actions, vec![EngineAction::Commit("á".into())]);
    }

    fn macro_engine(list: Vec<Macro>, delims: MacroDelimiters) -> (Engine, ContextId) {
        let (mut e, id) = engine();
        let table = MacroTable::try_new(list, 4096).unwrap();
        e.set_macros(Some(Arc::new(table)), delims);
        (e, id)
    }

    fn mac(id: &str, trigger: &str, expansion: &str) -> Macro {
        let mut m = Macro::new(trigger, expansion);
        m.id = id.into();
        m
    }

    #[test]
    fn macro_expands_at_each_enabled_delimiter() {
        let list = vec![mac("1", "vn", "Việt Nam")];
        let all = MacroDelimiters::all();
        for (key, delim_ok) in [
            (KeyInput::character(1, ' '), true),
            (KeyInput::new(1, LogicalKey::Enter), true),
            (KeyInput::new(1, LogicalKey::Tab), true),
            (KeyInput::character(1, '.'), true),
        ] {
            let (mut e, id) = macro_engine(list.clone(), all);
            type_str(&mut e, id, "vn");
            let out = e.process(KeyInput {
                context_id: id,
                ..key
            });
            assert!(!out.consumed, "the delimiter itself still reaches the app");
            assert_eq!(
                delim_ok,
                out.actions == vec![EngineAction::Commit("Việt Nam".into())]
            );
        }
    }

    #[test]
    fn macro_ignores_disabled_delimiters_and_non_delimiters() {
        let list = vec![mac("1", "vn", "Việt Nam")];
        let (mut e, id) = macro_engine(list.clone(), MacroDelimiters::SPACE);
        type_str(&mut e, id, "vn");
        let out = e.process(KeyInput::character(id, '.'));
        assert_eq!(out.actions, vec![EngineAction::Commit("vn".into())]);

        // Arrow keys, focus loss and digits never expand.
        let (mut e, id) = macro_engine(list.clone(), MacroDelimiters::all());
        type_str(&mut e, id, "vn");
        let out = e.process(KeyInput::new(id, LogicalKey::Left));
        assert_eq!(out.actions, vec![EngineAction::Commit("vn".into())]);
        type_str(&mut e, id, "vn");
        let out = e.notify(id, Notify::FocusOut);
        assert_eq!(out.actions, vec![EngineAction::Commit("vn".into())]);
        type_str(&mut e, id, "vn");
        let out = e.process(KeyInput::character(id, '1'));
        assert_eq!(out.actions, vec![EngineAction::Commit("vn".into())]);
    }

    #[test]
    fn macro_matches_composed_text_and_follows_case() {
        let (mut e, id) = macro_engine(vec![mac("1", "việt", "VIET!")], MacroDelimiters::all());
        type_str(&mut e, id, "vieejt");
        let out = e.process(KeyInput::character(id, ' '));
        assert_eq!(out.actions, vec![EngineAction::Commit("VIET!".into())]);

        let (mut e, id) = macro_engine(vec![mac("1", "vn", "Việt Nam")], MacroDelimiters::all());
        type_str(&mut e, id, "VN");
        let out = e.process(KeyInput::character(id, ' '));
        assert_eq!(out.actions, vec![EngineAction::Commit("VIỆT NAM".into())]);
    }

    #[test]
    fn macro_is_off_for_english_secure_and_disabled_contexts() {
        let list = vec![mac("1", "vn", "Việt Nam")];
        let (mut e, id) = macro_engine(list.clone(), MacroDelimiters::all());
        e.set_context_macros(id, false, None);
        type_str(&mut e, id, "vn");
        let out = e.process(KeyInput::character(id, ' '));
        assert_eq!(out.actions, vec![EngineAction::Commit("vn".into())]);

        let (mut e, id) = macro_engine(list.clone(), MacroDelimiters::all());
        e.set_mode(id, InputMode::English);
        for o in type_str(&mut e, id, "vn ") {
            assert_eq!(o, Output::default());
        }

        let (mut e, id) = macro_engine(list, MacroDelimiters::all());
        e.notify(
            id,
            Notify::FocusIn {
                caps: Capabilities::SECURE_INPUT,
            },
        );
        for o in type_str(&mut e, id, "vn ") {
            assert_eq!(o, Output::default());
        }
    }

    #[test]
    fn profile_scoped_macro_needs_the_profile() {
        let mut scoped = mac("1", "sig", "profile text");
        scoped.scope = ime_macro::Scope::Profile("p1".into());
        let list = vec![scoped, mac("2", "sig", "global text")];
        let (mut e, id) = macro_engine(list, MacroDelimiters::all());
        e.set_context_macros(id, true, Some("p1"));
        type_str(&mut e, id, "sig");
        let out = e.process(KeyInput::character(id, ' '));
        assert_eq!(
            out.actions,
            vec![EngineAction::Commit("profile text".into())]
        );
        e.set_context_macros(id, true, None);
        type_str(&mut e, id, "sig");
        let out = e.process(KeyInput::character(id, ' '));
        assert_eq!(
            out.actions,
            vec![EngineAction::Commit("global text".into())]
        );
    }

    #[test]
    fn would_consume_agrees_with_process_when_a_macro_fires() {
        let (mut e, id) = macro_engine(vec![mac("1", "vn", "Việt Nam")], MacroDelimiters::all());
        for c in "vn vn.".chars() {
            let ev = KeyInput::character(id, c);
            let predicted = e.would_consume(&ev);
            assert_eq!(predicted, e.process(ev).consumed, "{c}");
        }
    }

    #[test]
    fn per_context_config_overrides_the_global_one() {
        let (mut e, a) = engine();
        let b = e.create_context(ContextInfo::default());
        let vni = EngineConfig {
            scheme: Scheme::Vni,
            ..EngineConfig::default()
        };
        e.set_context_config(b, Some(Arc::new(vni)));
        type_str(&mut e, a, "tie6ng");
        type_str(&mut e, b, "tie6ng");
        let out_a = e.process(KeyInput::character(a, ' '));
        let out_b = e.process(KeyInput::character(b, ' '));
        // Telex ends the word at the digit, so only `ng` is left to commit.
        assert_eq!(out_a.actions, vec![EngineAction::Commit("ng".into())]);
        assert_eq!(out_b.actions, vec![EngineAction::Commit("tiêng".into())]);
        e.set_context_config(b, None);
        type_str(&mut e, b, "tie6ng");
        let out_b = e.process(KeyInput::character(b, ' '));
        assert_eq!(out_b.actions, vec![EngineAction::Commit("ng".into())]);
    }
}
