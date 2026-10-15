//! The IBus engine protocol on D-Bus (`org.freedesktop.IBus.Engine`, `.Factory`, `.Service`).
//! The interface definitions were checked against the XML that libibus 1.5.34 ships; older
//! daemons call a subset of the same methods (`FocusIn` without `FocusInId`, no `ContentType`).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use ime_runtime::ime_core::InputMode;
use ime_runtime::Runtime;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, Value};
use zbus::{fdo, interface, ObjectServer};

use crate::session::{Effect, Session};
use crate::trace::Trace;
use crate::wire;

pub use crate::COMPONENT_NAME;
pub const FACTORY_PATH: &str = "/org/freedesktop/IBus/Factory";
const ENGINE_PATH_PREFIX: &str = "/org/freedesktop/IBus/Engine/pkey";

/// `IBusPreeditFocusMode`: when the focus is lost the client commits the preedit instead of
/// dropping it, so a word being composed is never lost.
const PREEDIT_COMMIT: u32 = 1;

/// One runtime for the whole process: the VI/EN state and the configuration are shared by every
/// input context (spec §14.4).
pub struct Shared {
    rt: Mutex<Runtime>,
    trace: Trace,
}

impl Shared {
    pub fn new(rt: Runtime) -> Arc<Shared> {
        Arc::new(Shared {
            rt: Mutex::new(rt),
            trace: Trace::from_environment(),
        })
    }

    /// A panic inside the engine must never take the daemon's connection down with it.
    fn lock(&self) -> MutexGuard<'_, Runtime> {
        self.rt.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn mode_label(m: InputMode) -> (&'static str, &'static str) {
    match m {
        InputMode::Vietnamese => ("VI", "pKey: Tiếng Việt (bấm để chuyển sang English)"),
        InputMode::English => ("EN", "pKey: English (bấm để chuyển sang Tiếng Việt)"),
    }
}

struct Engine {
    shared: Arc<Shared>,
    session: Arc<Mutex<Session>>,
}

impl Engine {
    fn session(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Run `f` on the session and runtime; on a panic, log and fall back to `default`.
    fn run<T>(&self, default: T, f: impl FnOnce(&mut Session, &mut Runtime) -> T) -> T {
        let mut rt = self.shared.lock();
        let mut s = self.session();
        match catch_unwind(AssertUnwindSafe(|| f(&mut s, &mut rt))) {
            Ok(v) => v,
            Err(_) => {
                eprintln!("pkey-ibus: internal error, key passed through");
                default
            }
        }
    }

    async fn emit(&self, em: &SignalEmitter<'_>, effects: Vec<Effect>) -> zbus::Result<()> {
        let underline = self.shared.lock().config().compat.preedit_underline;
        for e in effects {
            let t = &self.shared.trace;
            if t.enabled() {
                match &e {
                    Effect::Commit(x) => {
                        t.log(format_args!("emit commit len={}", x.chars().count()))
                    }
                    Effect::Preedit { text, cursor } => t.log(format_args!(
                        "emit preedit len={} cursor={cursor}",
                        text.chars().count()
                    )),
                    Effect::HidePreedit => t.log(format_args!("emit hide-preedit")),
                    Effect::Replace { delete, text } => t.log(format_args!(
                        "emit replace delete={delete} len={}",
                        text.chars().count()
                    )),
                    Effect::Mode(m) => t.log(format_args!("emit mode {m:?}")),
                }
            }
            match e {
                Effect::Commit(t) => Engine::commit_text(em, wire::text(&t, false)).await?,
                Effect::Preedit { text, cursor } => {
                    Engine::update_preedit_text(
                        em,
                        wire::text(&text, underline),
                        cursor,
                        !text.is_empty(),
                        PREEDIT_COMMIT,
                    )
                    .await?
                }
                Effect::HidePreedit => Engine::hide_preedit_text(em).await?,
                Effect::Replace { delete, text } => {
                    if delete > 0 {
                        Engine::delete_surrounding_text(em, -(delete as i32), delete).await?;
                    }
                    if !text.is_empty() {
                        Engine::commit_text(em, wire::text(&text, false)).await?;
                    }
                }
                Effect::Mode(m) => {
                    let (label, tip) = mode_label(m);
                    Engine::update_property(em, wire::mode_property(label, tip)).await?
                }
            }
        }
        Ok(())
    }
}

#[interface(name = "org.freedesktop.IBus.Engine")]
impl Engine {
    async fn process_key_event(
        &self,
        keyval: u32,
        keycode: u32,
        state: u32,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>,
    ) -> bool {
        let r = self.run(None, |s, rt| {
            Some(s.process_key(rt, keyval, keycode, state))
        });
        let Some(r) = r else { return false };
        let t = &self.shared.trace;
        if t.enabled() {
            let ev = crate::keymap::build_input(0, keyval, keycode, state);
            t.log(format_args!(
                "key {:?} release={} state={state:#x} handled={} effects={}",
                ev.logical_key,
                state & crate::keymap::RELEASE_MASK != 0,
                r.handled,
                r.effects.len()
            ));
        }
        if self.emit(&em, r.effects).await.is_err() {
            return false;
        }
        r.handled
    }

    async fn set_cursor_location(&self, _x: i32, _y: i32, _w: i32, _h: i32) {}

    async fn process_hand_writing_event(&self, _coordinates: Vec<f64>) {}

    async fn cancel_hand_writing(&self, _n_strokes: u32) {}

    async fn set_capabilities(&self, caps: u32) {
        self.shared
            .trace
            .log(format_args!("set-capabilities {caps:#x}"));
        self.run((), |s, rt| s.set_capabilities(rt, caps));
    }

    async fn property_activate(
        &self,
        name: &str,
        _state: u32,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>,
    ) {
        if name == crate::MODE_PROPERTY {
            let fx = self.run(Vec::new(), |s, rt| s.toggle_mode(rt));
            let _ = self.emit(&em, fx).await;
        }
    }

    async fn property_show(&self, _name: &str) {}

    async fn property_hide(&self, _name: &str) {}

    async fn candidate_clicked(&self, _index: u32, _button: u32, _state: u32) {}

    async fn focus_in(&self, #[zbus(signal_emitter)] em: SignalEmitter<'_>) {
        self.focus(em, None).await;
    }

    async fn focus_in_id(
        &self,
        _object_path: &str,
        client: &str,
        #[zbus(signal_emitter)] em: SignalEmitter<'_>,
    ) {
        self.focus(em, Some(client)).await;
    }

    async fn focus_out(&self) {
        self.shared.trace.log(format_args!("focus-out"));
        self.run((), |s, rt| s.focus_out(rt));
    }

    async fn focus_out_id(&self, _object_path: &str) {
        self.run((), |s, rt| s.focus_out(rt));
    }

    async fn reset(&self, #[zbus(signal_emitter)] em: SignalEmitter<'_>) {
        self.shared
            .trace
            .log(format_args!("reset shown={}", self.session().shown_len()));
        let fx = self.run(Vec::new(), |s, rt| s.reset(rt));
        let _ = self.emit(&em, fx).await;
    }

    async fn enable(&self, #[zbus(signal_emitter)] em: SignalEmitter<'_>) {
        self.announce_properties(&em).await;
    }

    async fn disable(&self) {
        self.run((), |s, rt| s.focus_out(rt));
    }

    async fn page_up(&self) {}

    async fn page_down(&self) {}

    async fn cursor_up(&self) {}

    async fn cursor_down(&self) {}

    async fn set_surrounding_text(&self, text: Value<'_>, cursor_pos: u32, anchor_pos: u32) {
        let t = &self.shared.trace;
        if t.enabled() {
            let text = wire::text_of(&text).unwrap_or_default();
            let s = self.session();
            t.log(format_args!(
                "surrounding len={} cursor={cursor_pos} anchor={anchor_pos} shown={} ends-with-shown={}",
                text.chars().count(),
                s.shown_len(),
                s.shown_matches(&text, cursor_pos as usize)
            ));
        }
    }

    async fn panel_extension_received(&self, _event: Value<'_>) {}

    async fn panel_extension_register_keys(&self, _data: Value<'_>) {}

    #[zbus(property)]
    async fn set_content_type(&self, value: (u32, u32)) {
        self.shared.trace.log(format_args!(
            "content-type purpose={} hints={:#x}",
            value.0, value.1
        ));
        self.run((), |s, rt| s.set_content_type(rt, value.0));
    }

    /// Tells the daemon to use `FocusInId`, which carries the name of the client application.
    #[zbus(property)]
    fn focus_id(&self) -> (bool,) {
        (true,)
    }

    #[zbus(property)]
    fn active_surrounding_text(&self) -> (bool,) {
        (false,)
    }

    #[zbus(signal)]
    async fn commit_text(em: &SignalEmitter<'_>, text: Value<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn update_preedit_text(
        em: &SignalEmitter<'_>,
        text: Value<'_>,
        cursor_pos: u32,
        visible: bool,
        mode: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn delete_surrounding_text(
        em: &SignalEmitter<'_>,
        offset: i32,
        n_chars: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn hide_preedit_text(em: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn register_properties(em: &SignalEmitter<'_>, props: Value<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn update_property(em: &SignalEmitter<'_>, prop: Value<'_>) -> zbus::Result<()>;
}

impl Engine {
    async fn focus(&self, em: SignalEmitter<'_>, client: Option<&str>) {
        self.shared
            .trace
            .log(format_args!("focus-in client={}", client.unwrap_or("-")));
        let fx = self.run(Vec::new(), |s, rt| s.focus_in(rt, client));
        self.announce_properties(&em).await;
        let _ = self.emit(&em, fx).await;
    }

    async fn announce_properties(&self, em: &SignalEmitter<'_>) {
        let mode = {
            let rt = self.shared.lock();
            let id = self.session().id();
            rt.mode(id)
        };
        let (label, tip) = mode_label(mode);
        let props = wire::registered_properties(vec![wire::mode_property(label, tip)]);
        let _ = Engine::register_properties(em, props).await;
    }
}

/// `org.freedesktop.IBus.Service`: the daemon destroys the engine object when it switches away.
struct Service {
    shared: Arc<Shared>,
    session: Arc<Mutex<Session>>,
    path: String,
}

#[interface(name = "org.freedesktop.IBus.Service")]
impl Service {
    async fn destroy(&self, #[zbus(object_server)] server: &ObjectServer) {
        {
            let mut rt = self.shared.lock();
            self.session
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .destroy(&mut rt);
        }
        let _ = server.remove::<Engine, _>(self.path.as_str()).await;
        let _ = server.remove::<Service, _>(self.path.as_str()).await;
    }
}

pub struct Factory {
    pub shared: Arc<Shared>,
    next: AtomicU32,
}

impl Factory {
    pub fn new(shared: Arc<Shared>) -> Factory {
        Factory {
            shared,
            next: AtomicU32::new(1),
        }
    }
}

#[interface(name = "org.freedesktop.IBus.Factory")]
impl Factory {
    async fn create_engine(
        &self,
        name: &str,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        if name != "pkey" {
            return Err(fdo::Error::InvalidArgs(format!("unknown engine {name}")));
        }
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        let path = format!("{ENGINE_PATH_PREFIX}{n}");
        let session = {
            let mut rt = self.shared.lock();
            Arc::new(Mutex::new(Session::new(&mut rt)))
        };
        let engine = Engine {
            shared: self.shared.clone(),
            session: session.clone(),
        };
        let service = Service {
            shared: self.shared.clone(),
            session,
            path: path.clone(),
        };
        server.at(path.as_str(), engine).await?;
        server.at(path.as_str(), service).await?;
        OwnedObjectPath::try_from(path).map_err(|e| fdo::Error::Failed(e.to_string()))
    }
}
