use std::sync::Arc;

use ime_core::{
    Capabilities, ContextId, ContextInfo, Engine, EngineAction, EngineConfig, KeyInput, LogicalKey,
    Notify, Output,
};

/// One step of a scripted typing session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Key {
        logical: LogicalKey,
        text: Option<char>,
    },
    Notify(Notify),
}

/// Parse a key script. Printable characters are typed as they are; a space is the Space key.
/// Special keys are written `{BS} {ESC} {ENTER} {TAB} {DEL} {LEFT} {RIGHT} {UP} {DOWN} {HOME}
/// {END} {PGUP} {PGDN} {F1}`, host events `{FOCUSIN} {FOCUSOUT} {CARET} {SELECT} {RESET}
/// {HOSTCANCEL}`, and `{LBRACE}` types a literal `{`.
pub fn parse_keys(script: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    let mut chars = script.chars();
    while let Some(c) = chars.next() {
        if c != '{' {
            let logical = if c == ' ' {
                LogicalKey::Space
            } else {
                LogicalKey::Char
            };
            steps.push(Step::Key {
                logical,
                text: Some(c),
            });
            continue;
        }
        let mut name = String::new();
        loop {
            match chars.next() {
                Some('}') => break,
                Some(ch) => name.push(ch),
                None => return Err(format!("unterminated token '{{{name}'")),
            }
        }
        let key = |logical| Step::Key {
            logical,
            text: None,
        };
        let step = match name.to_ascii_uppercase().as_str() {
            "BS" => key(LogicalKey::Backspace),
            "ESC" => key(LogicalKey::Escape),
            "ENTER" => key(LogicalKey::Enter),
            "TAB" => key(LogicalKey::Tab),
            "DEL" => key(LogicalKey::Delete),
            "LEFT" => key(LogicalKey::Left),
            "RIGHT" => key(LogicalKey::Right),
            "UP" => key(LogicalKey::Up),
            "DOWN" => key(LogicalKey::Down),
            "HOME" => key(LogicalKey::Home),
            "END" => key(LogicalKey::End),
            "PGUP" => key(LogicalKey::PageUp),
            "PGDN" => key(LogicalKey::PageDown),
            "F1" => key(LogicalKey::Other),
            "LBRACE" => Step::Key {
                logical: LogicalKey::Char,
                text: Some('{'),
            },
            "FOCUSIN" => Step::Notify(Notify::FocusIn {
                caps: Capabilities::PREEDIT,
            }),
            "FOCUSOUT" => Step::Notify(Notify::FocusOut),
            "CARET" => Step::Notify(Notify::CaretMoved),
            "SELECT" => Step::Notify(Notify::SelectionChanged),
            "RESET" => Step::Notify(Notify::Reset),
            "HOSTCANCEL" => Step::Notify(Notify::CompositionTerminatedByHost),
            other => return Err(format!("unknown token '{{{other}}}'")),
        };
        steps.push(step);
    }
    Ok(steps)
}

/// A text field with one caret at the end: `doc` is committed text, `preedit` the composition.
///
/// It applies actions the way a real host would and records every contract violation
/// (spec §5, §7.2) in `violations` instead of panicking, so property tests can report them.
pub struct FakeHost {
    engine: Engine,
    ctx: ContextId,
    pub doc: String,
    pub preedit: Option<String>,
    pub violations: Vec<String>,
}

impl FakeHost {
    pub fn new(cfg: EngineConfig) -> Self {
        let mut engine = Engine::new(Arc::new(cfg));
        let ctx = engine.create_context(ContextInfo::default());
        FakeHost {
            engine,
            ctx,
            doc: String::new(),
            preedit: None,
            violations: Vec::new(),
        }
    }

    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn context(&self) -> ContextId {
        self.ctx
    }

    /// Committed text plus the composition currently shown.
    pub fn visible(&self) -> String {
        format!("{}{}", self.doc, self.preedit.as_deref().unwrap_or(""))
    }

    pub fn feed(&mut self, script: &str) -> Result<(), String> {
        for step in parse_keys(script)? {
            self.step(step);
        }
        Ok(())
    }

    pub fn step(&mut self, step: Step) {
        match step {
            Step::Key { logical, text } => {
                let mut ev = KeyInput::new(self.ctx, logical);
                ev.text = text;
                self.key(ev);
            }
            Step::Notify(n) => {
                let out = self.engine.notify(self.ctx, n);
                self.apply(&out);
                if matches!(
                    n,
                    Notify::CompositionTerminatedByHost | Notify::HostCommitFailed
                ) {
                    // the host dropped its own composition too
                    self.preedit = None;
                }
            }
        }
    }

    pub fn key(&mut self, ev: KeyInput) {
        let predicted = self.engine.would_consume(&ev);
        let out = self.engine.process(ev);
        if predicted != out.consumed {
            self.violations
                .push("would_consume disagrees with process".into());
        }
        if out.consumed && ev.logical_key == LogicalKey::Char && self.preedit.is_none() {
            // a consumed character key must leave something in the preedit unless it committed
            let committed = out
                .actions
                .iter()
                .any(|a| matches!(a, EngineAction::Commit(_)));
            if !committed && !out.actions.iter().any(is_preedit) {
                self.violations
                    .push("consumed char key without any preedit update".into());
            }
        }
        self.apply(&out);
        if !out.consumed {
            self.pass_through(&ev);
        }
    }

    fn apply(&mut self, out: &Output) {
        for action in &out.actions {
            match action {
                EngineAction::UpdatePreedit { text, .. } => {
                    if text.is_empty() {
                        self.violations.push("empty preedit".into());
                    }
                    self.preedit = Some(text.clone());
                }
                EngineAction::Commit(text) => {
                    self.doc.push_str(text);
                    self.preedit = None;
                }
                EngineAction::CancelComposition => {
                    if self.preedit.is_none() {
                        self.violations.push("cancel without composition".into());
                    }
                    self.preedit = None;
                }
                EngineAction::ModeChanged(_) => {}
            }
        }
    }

    /// The application handles a key the engine did not consume.
    fn pass_through(&mut self, ev: &KeyInput) {
        match ev.logical_key {
            LogicalKey::Char => {
                if let Some(c) = ev.text {
                    self.doc.push(c);
                }
            }
            LogicalKey::Space => self.doc.push(' '),
            LogicalKey::Enter => self.doc.push('\n'),
            LogicalKey::Tab => self.doc.push('\t'),
            LogicalKey::Backspace => {
                if self.preedit.is_some() {
                    self.violations
                        .push("Backspace reached the document while composing".into());
                }
                self.doc.pop();
            }
            _ => {}
        }
    }
}

fn is_preedit(a: &EngineAction) -> bool {
    matches!(a, EngineAction::UpdatePreedit { .. })
}
