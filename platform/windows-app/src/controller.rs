//! The decision logic of the standalone app, free of Win32 so it can be tested anywhere.
//!
//! A low-level keyboard hook sees each key before the application does. For every key the
//! controller says whether to swallow it and which edits to inject instead. Because the word is
//! typed into the application as the user goes (there is no TSF composition), every change of the
//! word is turned into "delete the part that changed, type the new part".

use std::collections::HashMap;

use ime_runtime::ime_config::AppIdentity;
use ime_runtime::ime_core::{
    Capabilities, ContextId, EngineAction, InputMode, Modifiers, Notify, Output,
};
use ime_runtime::{HotkeyAction, Runtime};

/// The window the keys are going to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Foreground {
    /// Identity of the window (an `HWND` value); a change means a different text field.
    pub window: u64,
    pub app: AppIdentity,
    /// A password box (or any field where the app must stay out of the way).
    pub secure: bool,
}

/// One keyboard hook event, already decoded.
#[derive(Clone, Copy, Debug)]
pub struct KbEvent {
    pub vk: u32,
    pub scan: u32,
    pub extended: bool,
    pub up: bool,
    /// What the layout produces for this press; `None` for non-text keys.
    pub text: Option<char>,
    pub modifiers: Modifiers,
}

/// An edit to inject into the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// Press Backspace this many times.
    Backspaces(usize),
    /// Type this text (as Unicode characters, independent of the keyboard layout).
    Text(String),
    /// Type an invisible character and delete it again. Browsers fill the rest of what is typed
    /// into the address bar with a selected suggestion; the first Backspace would delete that
    /// suggestion instead of a letter. This clears it without touching the text.
    ClearAutocomplete,
    /// Send the key that was swallowed, after the edits before it.
    ReplayKey,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Decision {
    /// Do not deliver the original event to the application.
    pub swallow: bool,
    pub ops: Vec<Op>,
    /// The Vietnamese/English mode changed because of this key.
    pub mode_changed: Option<InputMode>,
    pub hotkey: Option<HotkeyAction>,
}

struct Ctx {
    id: ContextId,
    secure: bool,
}

/// Programs whose address bar or search boxes autocomplete inline.
const AUTOCOMPLETE_APPS: &[&str] = &[
    "chrome.exe",
    "msedge.exe",
    "brave.exe",
    "vivaldi.exe",
    "opera.exe",
    "firefox.exe",
    "arc.exe",
    "chromium.exe",
];

/// Put `ClearAutocomplete` before the first Backspace when the application needs it. Appending
/// text needs nothing: typing over the selected suggestion replaces it.
pub fn adapt_for_app(ops: &mut Vec<Op>, app: &AppIdentity) {
    let needs = app
        .exe_name
        .as_deref()
        .is_some_and(|n| AUTOCOMPLETE_APPS.iter().any(|a| a.eq_ignore_ascii_case(n)));
    if !needs {
        return;
    }
    if let Some(i) = ops.iter().position(|o| matches!(o, Op::Backspaces(_))) {
        ops.insert(i, Op::ClearAutocomplete);
    }
}

pub struct Controller {
    rt: Runtime,
    contexts: HashMap<u64, Ctx>,
    /// What the engine's current word looks like in the application, per context.
    shown: HashMap<ContextId, String>,
    current: Option<u64>,
    last_down: Option<u32>,
    /// The whole app is switched off (tray menu "pause").
    pub paused: bool,
}

/// Edits that turn `shown` into `new` with as few key presses as possible.
pub fn edit_ops(shown: &str, new: &str) -> Vec<Op> {
    let common = shown
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let byte = |s: &str, n: usize| s.char_indices().nth(n).map_or(s.len(), |(i, _)| i);
    let removed = &shown[byte(shown, common)..];
    let added = &new[byte(new, common)..];
    let mut ops = Vec::new();
    if !removed.is_empty() {
        ops.push(Op::Backspaces(
            ime_unicode::caret_at_end(removed).graphemes as usize,
        ));
    }
    if !added.is_empty() {
        ops.push(Op::Text(added.to_string()));
    }
    ops
}

impl Controller {
    pub fn new(rt: Runtime) -> Controller {
        Controller {
            rt,
            contexts: HashMap::new(),
            shown: HashMap::new(),
            current: None,
            last_down: None,
            paused: false,
        }
    }

    pub fn runtime(&self) -> &Runtime {
        &self.rt
    }

    pub fn runtime_mut(&mut self) -> &mut Runtime {
        &mut self.rt
    }

    fn caps(secure: bool) -> Capabilities {
        let mut caps = Capabilities::PREEDIT;
        if secure {
            caps |= Capabilities::SECURE_INPUT;
        }
        caps
    }

    /// Forget the word being typed without touching the application: the focus moved, the mouse
    /// was clicked, or the application changed the text behind our back.
    pub fn drop_word(&mut self) {
        if let Some(c) = self.current.and_then(|w| self.contexts.get(&w)) {
            let _ = self.rt.notify(c.id, Notify::CompositionTerminatedByHost);
            self.shown.remove(&c.id);
        }
    }

    /// The window the user is typing into may have changed; returns its context.
    fn context_for(&mut self, fg: &Foreground) -> ContextId {
        if self.current != Some(fg.window) {
            self.drop_word();
            self.current = Some(fg.window);
            self.rt.reset_hotkeys();
            self.rt.poll_reload();
            if self.contexts.len() > 64 {
                for (_, c) in self.contexts.drain() {
                    self.rt.destroy_context(c.id);
                    self.shown.remove(&c.id);
                }
            }
        }
        let caps = Self::caps(fg.secure);
        let rt = &mut self.rt;
        let ctx = self.contexts.entry(fg.window).or_insert_with(|| Ctx {
            id: rt.create_context(fg.app.clone(), caps),
            secure: fg.secure,
        });
        if ctx.secure != fg.secure {
            ctx.secure = fg.secure;
            // The word, if any, stays as it is; the engine only stops reacting.
            self.shown.remove(&ctx.id);
            let _ = rt.update_context(ctx.id, fg.app.clone(), caps);
        }
        ctx.id
    }

    /// Turn the engine's actions into edits, tracking what the application now shows.
    fn translate(&mut self, id: ContextId, out: &Output, decision: &mut Decision) {
        for a in &out.actions {
            let shown = self.shown.entry(id).or_default();
            match a {
                EngineAction::UpdatePreedit { text, .. } => {
                    decision.ops.extend(edit_ops(shown, text));
                    *shown = text.clone();
                }
                EngineAction::Commit(text) => {
                    decision.ops.extend(edit_ops(shown, text));
                    shown.clear();
                }
                EngineAction::CancelComposition => {
                    decision.ops.extend(edit_ops(shown, ""));
                    shown.clear();
                }
                EngineAction::ModeChanged(m) => decision.mode_changed = Some(*m),
            }
        }
    }

    /// The user switched Vietnamese/English from the tray. Returns the new mode.
    pub fn toggle_mode(&mut self) -> InputMode {
        self.drop_word();
        let id = self
            .current
            .and_then(|w| self.contexts.get(&w))
            .map(|c| c.id);
        let now = id.map_or_else(|| self.rt.global_mode(), |id| self.rt.mode(id));
        let next = match now {
            InputMode::Vietnamese => InputMode::English,
            InputMode::English => InputMode::Vietnamese,
        };
        self.rt.set_global_mode(next);
        if let Some(id) = id {
            let _ = self.rt.set_mode(id, next);
        }
        next
    }

    /// The focus moved to `fg` (no key pressed yet). Returns the mode that applies there.
    pub fn focus(&mut self, fg: &Foreground) -> InputMode {
        let id = self.context_for(fg);
        let _ = self.rt.sync_mode(id);
        self.rt.mode(id)
    }

    /// Mouse button pressed: the caret may have moved anywhere.
    pub fn pointer_down(&mut self) {
        self.drop_word();
    }

    /// Decide what to do with one key event.
    pub fn key(&mut self, ev: &KbEvent, fg: &Foreground) -> Decision {
        let mut d = self.key_inner(ev, fg);
        adapt_for_app(&mut d.ops, &fg.app);
        d
    }

    fn key_inner(&mut self, ev: &KbEvent, fg: &Foreground) -> Decision {
        let mut d = Decision::default();
        let id = self.context_for(fg);
        let repeat = !ev.up && self.last_down == Some(ev.vk);
        self.last_down = if ev.up { None } else { Some(ev.vk) };
        let input = ime_winkeys::build_input_raw(
            id,
            ev.vk,
            ev.scan,
            ev.extended,
            ev.up,
            repeat,
            if ev.up { None } else { ev.text },
            ev.modifiers,
        );

        // Hotkeys first: they work while paused too, so the user can always get back.
        let fired = self.rt.feed_hotkeys(&input);
        if fired.any() {
            let outcome = self.rt.run_hotkeys(id, fired);
            self.translate(id, &outcome.output, &mut d);
            match outcome.hotkey {
                Some(HotkeyAction::CycleScheme) => {
                    self.rt.cycle_scheme();
                }
                other => d.hotkey = other,
            }
            d.swallow = outcome.output.consumed;
            if !d.swallow && !d.ops.is_empty() {
                d.swallow = true;
                d.ops.push(Op::ReplayKey);
            }
            return d;
        }
        if ev.up || self.paused {
            return d;
        }

        // Catch up with a mode toggled while another window had the focus.
        let synced = self.rt.sync_mode(id);
        self.translate(id, &synced, &mut d);

        let out = self.rt.process_engine(input);
        self.translate(id, &out, &mut d);
        d.swallow = out.consumed;
        if !d.swallow && !d.ops.is_empty() {
            // E.g. Space after a word that had to be restored: fix the word, then press Space.
            d.swallow = true;
            d.ops.push(Op::ReplayKey);
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ime_runtime::ime_config::Config;

    fn controller() -> Controller {
        Controller::new(Runtime::new(Config::default(), Vec::new()))
    }

    fn fg(window: u64) -> Foreground {
        Foreground {
            window,
            app: AppIdentity {
                exe_name: Some("app.exe".into()),
                ..Default::default()
            },
            secure: false,
        }
    }

    fn letter(c: char) -> KbEvent {
        KbEvent {
            vk: c.to_ascii_uppercase() as u32,
            scan: 0x1E,
            extended: false,
            up: false,
            text: Some(c),
            modifiers: Modifiers::empty(),
        }
    }

    fn special(vk: u32, text: Option<char>) -> KbEvent {
        KbEvent {
            vk,
            scan: 0,
            extended: false,
            up: false,
            text,
            modifiers: Modifiers::empty(),
        }
    }

    fn modkey(vk: u32, scan: u32, up: bool) -> KbEvent {
        KbEvent {
            vk,
            scan,
            extended: false,
            up,
            text: None,
            modifiers: Modifiers::empty(),
        }
    }

    /// A tiny application: applies the injected edits and the keys that were not swallowed.
    struct App {
        text: String,
    }

    impl App {
        fn apply(&mut self, ev: &KbEvent, d: &Decision) {
            for op in &d.ops {
                match op {
                    Op::Backspaces(n) => {
                        for _ in 0..*n {
                            self.text.pop();
                        }
                    }
                    Op::Text(t) => self.text.push_str(t),
                    // Types U+202F and deletes it: no net change to the text.
                    Op::ClearAutocomplete => {}
                    Op::ReplayKey => self.deliver(ev),
                }
            }
            if !d.swallow {
                self.deliver(ev);
            }
        }

        fn deliver(&mut self, ev: &KbEvent) {
            if ev.up {
                return;
            }
            match (ev.vk, ev.text) {
                (0x08, _) => {
                    self.text.pop();
                }
                (_, Some(c)) => self.text.push(c),
                _ => {}
            }
        }
    }

    fn type_into(c: &mut Controller, app: &mut App, fg: &Foreground, s: &str) {
        for ch in s.chars() {
            let ev = if ch == ' ' {
                special(0x20, Some(' '))
            } else {
                letter(ch)
            };
            let d = c.key(&ev, fg);
            app.apply(&ev, &d);
            let mut up = ev;
            up.up = true;
            let d = c.key(&up, fg);
            app.apply(&up, &d);
        }
    }

    #[test]
    fn edit_ops_are_minimal() {
        assert_eq!(edit_ops("", "t"), [Op::Text("t".into())]);
        assert_eq!(
            edit_ops("tie", "tiê"),
            [Op::Backspaces(1), Op::Text("ê".into())]
        );
        assert_eq!(
            edit_ops("tiêng", "tiếng"),
            [Op::Backspaces(3), Op::Text("ếng".into())]
        );
        assert_eq!(edit_ops("abc", "abc"), []);
        assert_eq!(edit_ops("abc", "ab"), [Op::Backspaces(1)]);
        assert_eq!(edit_ops("abc", ""), [Op::Backspaces(3)]);
        assert_eq!(edit_ops("", ""), []);
    }

    #[test]
    fn types_vietnamese_as_edits_of_the_word() {
        let (mut c, mut app, f) = (
            controller(),
            App {
                text: String::new(),
            },
            fg(1),
        );
        type_into(&mut c, &mut app, &f, "tieengs vieejt ");
        assert_eq!(app.text, "tiếng việt ");
    }

    #[test]
    fn plain_keys_are_not_swallowed() {
        let mut c = controller();
        let f = fg(1);
        // A space with nothing to commit, digits and punctuation are the application's.
        for ev in [
            special(0x20, Some(' ')),
            special(0x31, Some('1')),
            special(0xBE, Some('.')),
        ] {
            let d = c.key(&ev, &f);
            assert_eq!(d, Decision::default(), "{ev:?}");
        }
    }

    #[test]
    fn a_word_that_is_not_vietnamese_is_restored_on_space() {
        let (mut c, mut app, f) = (
            controller(),
            App {
                text: String::new(),
            },
            fg(1),
        );
        type_into(&mut c, &mut app, &f, "windows ");
        assert_eq!(app.text, "windows ");
    }

    #[test]
    fn backspace_edits_the_word_not_the_text_before_it() {
        let (mut c, mut app, f) = (
            controller(),
            App {
                text: String::new(),
            },
            fg(1),
        );
        type_into(&mut c, &mut app, &f, "xin tooi");
        assert_eq!(app.text, "xin tôi");
        let bs = special(0x08, None);
        let d = c.key(&bs, &f);
        assert!(d.swallow);
        app.apply(&bs, &d);
        assert_eq!(app.text, "xin tô");
        // The word is gone after the last letter, and the next Backspace is the application's.
        for _ in 0..2 {
            let d = c.key(&bs, &f);
            app.apply(&bs, &d);
        }
        assert_eq!(app.text, "xin ");
        let d = c.key(&bs, &f);
        assert!(!d.swallow, "no word: Backspace belongs to the application");
    }

    #[test]
    fn escape_gives_back_the_raw_keys() {
        let (mut c, mut app, f) = (
            controller(),
            App {
                text: String::new(),
            },
            fg(1),
        );
        type_into(&mut c, &mut app, &f, "tieeng");
        assert_eq!(app.text, "tiêng");
        let esc = special(0x1B, None);
        let d = c.key(&esc, &f);
        app.apply(&esc, &d);
        assert_eq!(app.text, "tieeng");
    }

    #[test]
    fn ctrl_shift_toggles_english_and_back() {
        let (mut c, mut app, f) = (
            controller(),
            App {
                text: String::new(),
            },
            fg(1),
        );
        let toggle = |c: &mut Controller, f: &Foreground| {
            let mut last = Decision::default();
            for (vk, scan, up) in [
                (0x11, 0x1D, false),
                (0x10, 0x2A, false),
                (0x10, 0x2A, true),
                (0x11, 0x1D, true),
            ] {
                last = c.key(&modkey(vk, scan, up), f);
                assert!(!last.swallow, "modifier keys always reach the application");
            }
            last
        };
        let d = toggle(&mut c, &f);
        assert_eq!(d.mode_changed, Some(InputMode::English));
        type_into(&mut c, &mut app, &f, "tieengs ");
        assert_eq!(app.text, "tieengs ");
        let d = toggle(&mut c, &f);
        assert_eq!(d.mode_changed, Some(InputMode::Vietnamese));
        type_into(&mut c, &mut app, &f, "tieengs ");
        assert_eq!(app.text, "tieengs tiếng ");
    }

    #[test]
    fn ctrl_shift_with_a_key_in_between_does_not_toggle() {
        let mut c = controller();
        let f = fg(1);
        for ev in [
            modkey(0x11, 0x1D, false),
            modkey(0x10, 0x2A, false),
            KbEvent {
                vk: 0x54,
                scan: 0x14,
                extended: false,
                up: false,
                text: None,
                modifiers: Modifiers::CTRL | Modifiers::SHIFT,
            },
            modkey(0x10, 0x2A, true),
            modkey(0x11, 0x1D, true),
        ] {
            let d = c.key(&ev, &f);
            assert_eq!(d.mode_changed, None);
        }
    }

    #[test]
    fn the_mode_is_shared_between_windows() {
        let (mut c, mut app) = (
            controller(),
            App {
                text: String::new(),
            },
        );
        let (a, b) = (fg(1), fg(2));
        type_into(&mut c, &mut app, &a, "x");
        for (vk, scan, up) in [
            (0x11, 0x1D, false),
            (0x10, 0x2A, false),
            (0x10, 0x2A, true),
            (0x11, 0x1D, true),
        ] {
            c.key(&modkey(vk, scan, up), &a);
        }
        let mut other = App {
            text: String::new(),
        };
        type_into(&mut c, &mut other, &b, "vieejt ");
        assert_eq!(
            other.text, "vieejt ",
            "the other window follows the shared mode"
        );
    }

    #[test]
    fn changing_window_forgets_the_word() {
        let (mut c, mut app) = (
            controller(),
            App {
                text: String::new(),
            },
        );
        type_into(&mut c, &mut app, &fg(1), "tie");
        // Switch to another window and type a tone key there: it must not rewrite "tie".
        let mut other = App {
            text: String::new(),
        };
        type_into(&mut c, &mut other, &fg(2), "x");
        assert_eq!(other.text, "x");
    }

    #[test]
    fn a_mouse_click_forgets_the_word() {
        let (mut c, mut app, f) = (
            controller(),
            App {
                text: String::new(),
            },
            fg(1),
        );
        type_into(&mut c, &mut app, &f, "tie");
        c.pointer_down();
        app.text.clear(); // the user clicked somewhere else
        type_into(&mut c, &mut app, &f, "x");
        assert_eq!(app.text, "x");
    }

    #[test]
    fn password_fields_are_left_alone() {
        let (mut c, mut app) = (
            controller(),
            App {
                text: String::new(),
            },
        );
        let mut f = fg(1);
        f.secure = true;
        type_into(&mut c, &mut app, &f, "tieengs");
        assert_eq!(app.text, "tieengs");
        f.secure = false;
        type_into(&mut c, &mut app, &f, " tieengs ");
        assert_eq!(app.text, "tieengs tiếng ");
    }

    #[test]
    fn paused_means_hands_off_but_hotkeys_still_work() {
        let (mut c, mut app, f) = (
            controller(),
            App {
                text: String::new(),
            },
            fg(1),
        );
        c.paused = true;
        type_into(&mut c, &mut app, &f, "tieengs ");
        assert_eq!(app.text, "tieengs ");
    }

    #[test]
    fn browsers_get_the_autocomplete_fix_only_before_backspaces() {
        let mut c = controller();
        let mut f = fg(1);
        f.app.exe_name = Some("Chrome.exe".into());
        // `c` and `a` only append: no fix needed.
        for ch in ['c', 'a'] {
            let d = c.key(&letter(ch), &f);
            assert!(!d.ops.contains(&Op::ClearAutocomplete), "{ch}: {:?}", d.ops);
        }
        // `s` turns `ca` into `cá`: the suggestion is cleared before the Backspace.
        let d = c.key(&letter('s'), &f);
        assert_eq!(
            d.ops,
            [
                Op::ClearAutocomplete,
                Op::Backspaces(1),
                Op::Text("á".into())
            ]
        );
        // The same keys in another program are unchanged.
        let mut c2 = controller();
        let f2 = fg(2);
        c2.key(&letter('c'), &f2);
        c2.key(&letter('a'), &f2);
        let d = c2.key(&letter('s'), &f2);
        assert_eq!(d.ops, [Op::Backspaces(1), Op::Text("á".into())]);
    }

    #[test]
    fn tray_toggle_switches_the_mode_for_every_window() {
        let (mut c, mut app) = (
            controller(),
            App {
                text: String::new(),
            },
        );
        let (a, b) = (fg(1), fg(2));
        assert_eq!(c.focus(&a), InputMode::Vietnamese);
        assert_eq!(c.toggle_mode(), InputMode::English);
        type_into(&mut c, &mut app, &a, "tieengs ");
        assert_eq!(app.text, "tieengs ");
        assert_eq!(c.focus(&b), InputMode::English);
        assert_eq!(c.toggle_mode(), InputMode::Vietnamese);
        assert_eq!(c.focus(&a), InputMode::Vietnamese);
    }

    #[test]
    fn game_profiles_pass_everything_through() {
        let mut cfg = Config::default();
        cfg.profiles.push(ime_runtime::ime_config::Profile {
            id: "123e4567-e89b-12d3-a456-426614174000".into(),
            name: "game".into(),
            enabled: true,
            matcher: ime_runtime::ime_config::ProfileMatch {
                kind: ime_runtime::ime_config::MatchKind::ExeName,
                value: "game.exe".into(),
                pattern: ime_runtime::ime_config::MatchPattern::Exact,
            },
            overrides: ime_runtime::ime_config::ProfileOverride {
                game_mode: Some(true),
                ..Default::default()
            },
        });
        let mut c = Controller::new(Runtime::new(cfg, Vec::new()));
        let mut app = App {
            text: String::new(),
        };
        let mut f = fg(5);
        f.app.exe_name = Some("game.exe".into());
        type_into(&mut c, &mut app, &f, "tieengs ");
        assert_eq!(app.text, "tieengs ");
    }

    #[test]
    fn macros_expand_through_edits() {
        let mut m = ime_runtime::ime_core::Macro::new("vn", "Việt Nam");
        m.id = "1".into();
        let mut c = Controller::new(Runtime::new(Config::default(), vec![m]));
        let (mut app, f) = (
            App {
                text: String::new(),
            },
            fg(1),
        );
        type_into(&mut c, &mut app, &f, "vn ");
        assert_eq!(app.text, "Việt Nam ");
    }
}
