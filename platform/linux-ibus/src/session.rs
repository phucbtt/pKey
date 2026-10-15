//! One IBus engine object = one input context. This module turns IBus callbacks into engine
//! calls and engine output into the effects the D-Bus layer emits. It does not touch D-Bus, so
//! it is tested on any machine.

use ime_runtime::ime_config::AppIdentity;
use ime_runtime::ime_core::{Capabilities, ContextId, EngineAction, InputMode, Notify, Output};
use ime_runtime::{HotkeyAction, Runtime};

use crate::keymap;

/// `IBusCapabilite`.
const CAP_PREEDIT_TEXT: u32 = 1 << 0;
const CAP_SURROUNDING_TEXT: u32 = 1 << 5;

/// `GtkInputPurpose` values IBus forwards in `ContentType`.
const PURPOSE_PASSWORD: u32 = 8;
const PURPOSE_PIN: u32 = 9;
const PURPOSE_TERMINAL: u32 = 10;

/// What the D-Bus layer has to emit, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Commit(String),
    /// `cursor` counts characters.
    Preedit {
        text: String,
        cursor: u32,
    },
    HidePreedit,
    /// Inline mode: delete `delete` characters before the cursor (the word typed so far), then
    /// type `text` in their place. Nothing is drawn as preedit, so nothing is underlined.
    Replace {
        delete: u32,
        text: String,
    },
    /// The Vietnamese/English mode changed: refresh the indicator.
    Mode(InputMode),
}

pub struct Session {
    id: ContextId,
    ibus_caps: u32,
    content: Capabilities,
    app: AppIdentity,
    focused: bool,
    /// Inline mode: the part of the word that is already in the text field.
    shown: String,
}

/// `AppIdentity` for an IBus client name such as `gtk3-im:firefox` or `gnome-shell`.
pub fn identity_from_client(client: &str) -> AppIdentity {
    let program = client
        .split_once(':')
        .map(|(_, p)| p)
        .filter(|p| !p.is_empty());
    AppIdentity {
        program: program.map(str::to_string),
        ibus_client: (!client.is_empty()).then(|| client.to_string()),
        ..AppIdentity::default()
    }
}

/// Effects of an engine `Output`.
fn effects(out: &Output) -> Vec<Effect> {
    let mut v = Vec::new();
    for a in &out.actions {
        match a {
            EngineAction::UpdatePreedit { text, caret } => v.push(Effect::Preedit {
                text: text.clone(),
                cursor: caret.scalars,
            }),
            EngineAction::Commit(t) => {
                v.push(Effect::Commit(t.clone()));
                v.push(Effect::HidePreedit);
            }
            EngineAction::CancelComposition => v.push(Effect::HidePreedit),
            EngineAction::ModeChanged(m) => v.push(Effect::Mode(*m)),
        }
    }
    v
}

/// The same output for a client that gets the word typed straight into its text field.
fn inline_effects(shown: &mut String, out: &Output) -> Vec<Effect> {
    let mut v = Vec::new();
    let set = |shown: &mut String, text: &str, v: &mut Vec<Effect>| {
        if shown != text {
            v.push(Effect::Replace {
                delete: shown.chars().count() as u32,
                text: text.to_string(),
            });
            *shown = text.to_string();
        }
    };
    for a in &out.actions {
        match a {
            EngineAction::UpdatePreedit { text, .. } => set(shown, text, &mut v),
            EngineAction::Commit(t) => {
                set(shown, t, &mut v);
                shown.clear();
            }
            EngineAction::CancelComposition => set(shown, "", &mut v),
            EngineAction::ModeChanged(m) => v.push(Effect::Mode(*m)),
        }
    }
    v
}

pub struct KeyResult {
    /// The key was eaten; when false the application receives it after the effects.
    pub handled: bool,
    pub effects: Vec<Effect>,
}

impl Session {
    pub fn new(rt: &mut Runtime) -> Session {
        let mut s = Session {
            id: 0,
            ibus_caps: CAP_PREEDIT_TEXT,
            content: Capabilities::empty(),
            app: AppIdentity::default(),
            focused: false,
            shown: String::new(),
        };
        s.id = rt.create_context(s.app.clone(), s.caps());
        s
    }

    /// Characters of the word that are already typed into the text field (inline mode).
    pub fn shown_len(&self) -> usize {
        self.shown.chars().count()
    }

    /// Does the client's text before its cursor end with the word we think we typed? `cursor`
    /// counts characters.
    pub fn shown_matches(&self, text: &str, cursor: usize) -> bool {
        let before: Vec<char> = text.chars().take(cursor).collect();
        let want: Vec<char> = self.shown.chars().collect();
        before.ends_with(&want)
    }

    pub fn id(&self) -> ContextId {
        self.id
    }

    /// Type the word into the text field instead of showing it as preedit? Only when the user
    /// asked for it and the client can delete text around the cursor; terminals keep the preedit.
    fn inline(&self, rt: &Runtime) -> bool {
        !rt.config().compat.linux_preedit
            && self.ibus_caps & CAP_SURROUNDING_TEXT != 0
            && !self.content.contains(Capabilities::TERMINAL_HINT)
    }

    fn effects_of(&mut self, rt: &Runtime, out: &Output) -> Vec<Effect> {
        if self.inline(rt) {
            inline_effects(&mut self.shown, out)
        } else {
            self.shown.clear();
            effects(out)
        }
    }

    /// Engine capabilities from what the client declared (spec §14.6).
    fn caps(&self) -> Capabilities {
        let mut c = self.content;
        if self.ibus_caps & CAP_PREEDIT_TEXT != 0 {
            c |= Capabilities::PREEDIT;
        }
        if self.ibus_caps & CAP_SURROUNDING_TEXT != 0 {
            c |= Capabilities::SURROUNDING_TEXT;
        }
        c
    }

    pub fn process_key(
        &mut self,
        rt: &mut Runtime,
        keyval: u32,
        keycode: u32,
        state: u32,
    ) -> KeyResult {
        let ev = keymap::build_input(self.id, keyval, keycode, state);
        let outcome = rt.handle_key(ev);
        let fx = self.effects_of(rt, &outcome.output);
        match outcome.hotkey {
            Some(HotkeyAction::CycleScheme) => {
                rt.cycle_scheme();
            }
            // No settings program on Linux yet.
            Some(HotkeyAction::OpenSettings) | None => {}
        }
        // A key release is never ours: the application must see it, so that its own key state
        // stays balanced.
        let released = state & keymap::RELEASE_MASK != 0;
        KeyResult {
            handled: outcome.output.consumed && !released,
            effects: fx,
        }
    }

    pub fn focus_in(&mut self, rt: &mut Runtime, client: Option<&str>) -> Vec<Effect> {
        rt.poll_reload();
        if let Some(c) = client {
            self.app = identity_from_client(c);
        }
        self.focused = true;
        rt.reset_hotkeys();
        let out = rt.update_context(self.id, self.app.clone(), self.caps());
        let mut fx = self.effects_of(rt, &out);
        fx.push(Effect::Mode(rt.mode(self.id)));
        fx
    }

    /// The client already clears or commits its own preedit when it loses focus (the preedit
    /// mode is COMMIT), so the engine only forgets its state.
    pub fn focus_out(&mut self, rt: &mut Runtime) {
        self.focused = false;
        self.shown.clear();
        rt.reset_hotkeys();
        let _ = rt.notify(self.id, Notify::CompositionTerminatedByHost);
        let _ = rt.notify(self.id, Notify::FocusOut);
    }

    /// The client reset the context (cursor moved by mouse, for instance): finish the word.
    pub fn reset(&mut self, rt: &mut Runtime) -> Vec<Effect> {
        let out = rt.notify(self.id, Notify::Reset);
        self.effects_of(rt, &out)
    }

    pub fn set_capabilities(&mut self, rt: &mut Runtime, caps: u32) {
        self.ibus_caps = caps;
        self.refresh(rt);
    }

    pub fn set_content_type(&mut self, rt: &mut Runtime, purpose: u32) {
        self.content = match purpose {
            PURPOSE_PASSWORD | PURPOSE_PIN => Capabilities::SECURE_INPUT,
            PURPOSE_TERMINAL => Capabilities::TERMINAL_HINT,
            _ => Capabilities::empty(),
        };
        self.refresh(rt);
    }

    fn refresh(&mut self, rt: &mut Runtime) {
        if self.focused {
            // The composition, if any, stays: only the capabilities changed.
            let _ = rt.update_context(self.id, self.app.clone(), self.caps());
        }
    }

    /// Click on the VI/EN indicator.
    pub fn toggle_mode(&mut self, rt: &mut Runtime) -> Vec<Effect> {
        let next = match rt.mode(self.id) {
            InputMode::Vietnamese => InputMode::English,
            InputMode::English => InputMode::Vietnamese,
        };
        rt.set_global_mode(next);
        let out = rt.set_mode(self.id, next);
        let mut fx = self.effects_of(rt, &out);
        fx.push(Effect::Mode(next));
        fx
    }

    pub fn destroy(&mut self, rt: &mut Runtime) {
        rt.destroy_context(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ime_runtime::ime_config::Config;

    /// Press and release `keyval` like the daemon does, returning the effects of the press.
    fn tap(s: &mut Session, rt: &mut Runtime, keyval: u32) -> KeyResult {
        let r = s.process_key(rt, keyval, 0, 0);
        s.process_key(rt, keyval, 0, keymap::RELEASE_MASK);
        r
    }

    fn type_str(s: &mut Session, rt: &mut Runtime, text: &str) -> (String, Option<String>) {
        let mut committed = String::new();
        let mut preedit = None;
        for ch in text.chars() {
            let r = tap(s, rt, ch as u32);
            for e in r.effects {
                match e {
                    Effect::Commit(t) => committed.push_str(&t),
                    Effect::Preedit { text, .. } => preedit = Some(text),
                    Effect::HidePreedit => preedit = None,
                    Effect::Replace { .. } | Effect::Mode(_) => {}
                }
            }
            if !r.handled && ch == ' ' {
                committed.push(' ');
            }
        }
        (committed, preedit)
    }

    fn fresh() -> (Runtime, Session) {
        let mut rt = Runtime::new(Config::default(), Vec::new());
        let mut s = Session::new(&mut rt);
        s.focus_in(&mut rt, Some("gtk3-im:gedit"));
        (rt, s)
    }

    #[test]
    fn telex_word_is_composed_then_committed() {
        let (mut rt, mut s) = fresh();
        let (committed, preedit) = type_str(&mut s, &mut rt, "tieengs");
        assert_eq!(committed, "");
        assert_eq!(preedit.as_deref(), Some("tiếng"));
        let (committed, preedit) = type_str(&mut s, &mut rt, " ");
        assert_eq!(committed, "tiếng ");
        assert_eq!(preedit, None);
    }

    #[test]
    fn releases_are_not_handled() {
        let (mut rt, mut s) = fresh();
        let down = s.process_key(&mut rt, 'a' as u32, 30, 0);
        assert!(down.handled);
        let up = s.process_key(&mut rt, 'a' as u32, 30, keymap::RELEASE_MASK);
        assert!(!up.handled);
    }

    #[test]
    fn password_fields_pass_through() {
        let (mut rt, mut s) = fresh();
        s.set_content_type(&mut rt, PURPOSE_PASSWORD);
        let r = tap(&mut s, &mut rt, 'a' as u32);
        assert!(!r.handled);
        assert!(r.effects.is_empty());
    }

    #[test]
    fn toggle_to_english_commits_and_passes_keys() {
        let (mut rt, mut s) = fresh();
        type_str(&mut s, &mut rt, "vieejt");
        let fx = s.toggle_mode(&mut rt);
        assert!(fx.contains(&Effect::Commit("việt".into())));
        assert!(fx.contains(&Effect::Mode(InputMode::English)));
        let r = tap(&mut s, &mut rt, 'a' as u32);
        assert!(!r.handled);
    }

    #[test]
    fn reset_commits_the_word() {
        let (mut rt, mut s) = fresh();
        type_str(&mut s, &mut rt, "vieejt");
        assert!(s.reset(&mut rt).contains(&Effect::Commit("việt".into())));
    }

    #[test]
    fn focus_out_forgets_the_word() {
        let (mut rt, mut s) = fresh();
        type_str(&mut s, &mut rt, "vieejt");
        s.focus_out(&mut rt);
        s.focus_in(&mut rt, None);
        let (committed, preedit) = type_str(&mut s, &mut rt, "a");
        assert_eq!(committed, "");
        assert_eq!(preedit.as_deref(), Some("a"));
    }

    #[test]
    fn client_name_gives_a_program() {
        let id = identity_from_client("gtk3-im:firefox");
        assert_eq!(id.program.as_deref(), Some("firefox"));
        assert_eq!(id.ibus_client.as_deref(), Some("gtk3-im:firefox"));
        let id = identity_from_client("gnome-shell");
        assert_eq!(id.program, None);
    }

    #[test]
    fn ctrl_shortcuts_pass_through() {
        let (mut rt, mut s) = fresh();
        type_str(&mut s, &mut rt, "vie");
        let r = s.process_key(&mut rt, 'c' as u32, 46, 1 << 2);
        assert!(!r.handled);
        assert!(r.effects.contains(&Effect::Commit("vie".into())));
    }

    /// What the text field contains after applying inline effects like a client would.
    fn apply(field: &mut String, fx: &[Effect]) {
        for e in fx {
            if let Effect::Replace { delete, text } = e {
                for _ in 0..*delete {
                    field.pop();
                }
                field.push_str(text);
            }
        }
    }

    fn inline_fresh() -> (Runtime, Session) {
        let mut config = Config::default();
        config.compat.linux_preedit = false;
        let mut rt = Runtime::new(config, Vec::new());
        let mut s = Session::new(&mut rt);
        s.set_capabilities(&mut rt, CAP_PREEDIT_TEXT | CAP_SURROUNDING_TEXT);
        s.focus_in(&mut rt, Some("gtk3-im:code"));
        (rt, s)
    }

    #[test]
    fn inline_mode_types_the_word_without_preedit() {
        let (mut rt, mut s) = inline_fresh();
        let mut field = String::new();
        for ch in "tieengs".chars() {
            let r = tap(&mut s, &mut rt, ch as u32);
            assert!(!r
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Preedit { .. })));
            apply(&mut field, &r.effects);
        }
        assert_eq!(field, "tiếng");
        let r = tap(&mut s, &mut rt, ' ' as u32);
        apply(&mut field, &r.effects);
        // The space itself reaches the application; the word is already there.
        assert!(!r.handled);
        assert_eq!(field, "tiếng");
    }

    #[test]
    fn inline_mode_backspace_and_cancel() {
        let (mut rt, mut s) = inline_fresh();
        let mut field = String::new();
        for ch in "hoaf".chars() {
            apply(&mut field, &tap(&mut s, &mut rt, ch as u32).effects);
        }
        assert_eq!(field, "hoà");
        let r = tap(&mut s, &mut rt, 0xff08);
        apply(&mut field, &r.effects);
        assert_eq!(field, "hò");
    }

    #[test]
    fn inline_mode_needs_surrounding_text() {
        let mut config = Config::default();
        config.compat.linux_preedit = false;
        let mut rt = Runtime::new(config, Vec::new());
        let mut s = Session::new(&mut rt);
        s.set_capabilities(&mut rt, CAP_PREEDIT_TEXT);
        s.focus_in(&mut rt, None);
        let r = tap(&mut s, &mut rt, 'a' as u32);
        assert!(r
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Preedit { .. })));
    }

    #[test]
    fn inline_mode_forgets_the_word_on_focus_out() {
        let (mut rt, mut s) = inline_fresh();
        let mut field = String::new();
        for ch in "vie".chars() {
            apply(&mut field, &tap(&mut s, &mut rt, ch as u32).effects);
        }
        s.focus_out(&mut rt);
        s.focus_in(&mut rt, None);
        apply(&mut field, &tap(&mut s, &mut rt, 'a' as u32).effects);
        assert_eq!(field, "viea");
    }
}
