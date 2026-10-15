//! The text service object (spec §14.1): activation, key sinks, composition.
//!
//! Threading: TSF calls everything on the UI thread of the host application (STA), so the
//! per-thread state uses `Rc`/`RefCell`. The [`Runtime`] is shared by every UI thread of the
//! process behind a mutex.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard, OnceLock};

use ime_runtime::ime_config::AppIdentity;
use ime_runtime::ime_core::{
    Capabilities, ContextId, EngineAction, InputMode, Modifiers, Notify, Output,
};
use ime_runtime::{HotkeyAction, HotkeysFired, KeyOutcome, Runtime};
use windows::core::{implement, IUnknownImpl, Interface, Ref, Result, BOOL, GUID};
use windows::Win32::Foundation::{E_INVALIDARG, E_POINTER, FALSE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, GetKeyboardLayout, GetKeyboardState, ToUnicodeEx,
};
use windows::Win32::UI::TextServices::{
    CLSID_TF_CategoryMgr, IEnumTfDisplayAttributeInfo, ITfCategoryMgr, ITfComposition,
    ITfCompositionSink, ITfCompositionSink_Impl, ITfContext, ITfContextComposition,
    ITfDisplayAttributeInfo, ITfDisplayAttributeProvider, ITfDisplayAttributeProvider_Impl,
    ITfDocumentMgr, ITfInsertAtSelection, ITfKeyEventSink, ITfKeyEventSink_Impl, ITfKeystrokeMgr,
    ITfRange, ITfSource, ITfTextInputProcessorEx, ITfTextInputProcessorEx_Impl,
    ITfTextInputProcessor_Impl, ITfThreadMgr, ITfThreadMgrEventSink, ITfThreadMgrEventSink_Impl,
    GUID_PROP_ATTRIBUTE, TF_AE_NONE, TF_ANCHOR_END, TF_IAS_QUERYONLY, TF_SELECTION,
    TF_SELECTIONSTYLE,
};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowLongPtrW, GWL_STYLE};

use windows::Win32::UI::TextServices::{ITfEditRecord, ITfTextEditSink, ITfTextEditSink_Impl};

use windows::Win32::UI::TextServices::{
    ITfCompartment, ITfInputScope, InputScope, GUID_PROP_INPUTSCOPE, IS_PASSWORD, IS_PRIVATE,
};

use super::guids::{GUID_COMPARTMENT_MODE, GUID_DISPLAY_ATTRIBUTE_COMPOSITION};
use super::{display, fail, guard, session, LIVE_OBJECTS};
use crate::keymap;

const ES_PASSWORD: u32 = 0x20;

fn runtime() -> MutexGuard<'static, Runtime> {
    static RT: OnceLock<Mutex<Runtime>> = OnceLock::new();
    RT.get_or_init(|| Mutex::new(Runtime::from_default_paths()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The host process: `GetModuleFileNameW(NULL)`, split into name and path (spec §12.2).
fn app_identity() -> AppIdentity {
    static ID: OnceLock<AppIdentity> = OnceLock::new();
    ID.get_or_init(|| {
        let mut buf = [0u16; 1024];
        // SAFETY: `buf` is a valid writable buffer.
        let n = unsafe { GetModuleFileNameW(None, &mut buf) } as usize;
        if n == 0 || n >= buf.len() {
            return AppIdentity::default();
        }
        let path = String::from_utf16_lossy(&buf[..n]);
        let exe = path.rsplit(['\\', '/']).next().map(str::to_string);
        AppIdentity {
            exe_name: exe,
            process_path: Some(path),
            ..AppIdentity::default()
        }
    })
    .clone()
}

/// Best-effort password-field detection for classic Win32 edit controls. Modern apps (browsers,
/// UWP) report passwords through the TSF input scope, which is not read yet (spec §12.2).
fn focus_is_password_edit() -> bool {
    // SAFETY: plain Win32 queries on the calling thread's focus window.
    unsafe {
        let hwnd: HWND = GetFocus();
        if hwnd.0.is_null() {
            return false;
        }
        let mut class = [0u16; 32];
        let n = GetClassNameW(hwnd, &mut class) as usize;
        let name = String::from_utf16_lossy(&class[..n.min(class.len())]);
        let is_edit = name.eq_ignore_ascii_case("Edit") || name.starts_with("RichEdit");
        // `GetWindowLongPtrW` returns `isize` on x64 and `i32` on x86; the style bits are the low 32.
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        is_edit && style & ES_PASSWORD != 0
    }
}

fn current_caps(scope_is_secure: bool) -> Capabilities {
    let mut caps = Capabilities::PREEDIT | Capabilities::SELECTION_REPLACE;
    if scope_is_secure || focus_is_password_edit() {
        caps |= Capabilities::SECURE_INPUT;
    }
    caps
}

fn key_down(vk: u32) -> bool {
    // SAFETY: plain Win32 query.
    unsafe { GetKeyState(vk as i32) < 0 }
}

fn read_modifiers() -> Modifiers {
    let mut m = Modifiers::empty();
    if key_down(keymap::VK_SHIFT) {
        m |= Modifiers::SHIFT;
    }
    if key_down(keymap::VK_CONTROL) {
        m |= Modifiers::CTRL;
    }
    if key_down(keymap::VK_MENU) {
        m |= Modifiers::ALT;
    }
    if key_down(keymap::VK_LWIN) || key_down(keymap::VK_RWIN) {
        m |= Modifiers::META;
    }
    // SAFETY: plain Win32 query; the low bit is the toggle state.
    if unsafe { GetKeyState(0x14) } & 1 != 0 {
        m |= Modifiers::CAPS_LOCK;
    }
    // AltGr is reported as Ctrl+Alt with the right Alt held (0xA5 = VK_RMENU, 0xA2 = VK_LCONTROL).
    if key_down(0xA5) && key_down(0xA2) {
        m |= Modifiers::ALT_GR;
    }
    m
}

/// What the keyboard layout types for this key press, if it is a character.
fn translate_char(vk: u32, lparam: isize) -> Option<char> {
    let scan = ((lparam as u32) >> 16) & 0xFF;
    let mut state = [0u8; 256];
    let mut buf = [0u16; 8];
    // SAFETY: both buffers are valid for the lengths implied by their types. Flag 0x4 keeps
    // `ToUnicodeEx` from altering the keyboard state (dead keys), which the application owns.
    let n = unsafe {
        GetKeyboardState(&mut state).ok()?;
        let hkl = GetKeyboardLayout(0);
        ToUnicodeEx(vk, scan, &state, &mut buf, 0x4, Some(hkl))
    };
    if n == 1 {
        char::from_u32(u32::from(buf[0]))
    } else {
        // 0: no character; negative: dead key; more: a dead key plus a character. Not ours.
        None
    }
}

fn is_modifier_vk(vk: u32) -> bool {
    matches!(
        vk,
        keymap::VK_SHIFT
            | keymap::VK_CONTROL
            | keymap::VK_MENU
            | keymap::VK_LWIN
            | keymap::VK_RWIN
            | 0xA0..=0xA5
    )
}

fn identity_of(ctx: &ITfContext) -> usize {
    ctx.cast::<windows::core::IUnknown>()
        .map_or(0, |u| u.as_raw() as usize)
}

struct CtxState {
    id: ContextId,
    /// Keeps the context alive so its address (our map key) cannot be reused by another one.
    #[allow(dead_code)]
    ctx: ITfContext,
    composition: Option<ITfComposition>,
    caps: Capabilities,
    /// Cookie of our `ITfTextEditSink` on this context (0 = not advised).
    edit_cookie: u32,
}

/// A decision made in the "test" half of a key event, carried out in the "do" half.
struct Pending {
    context: ContextId,
    outcome: KeyOutcome,
}

/// State shared with edit-session closures.
struct Shared {
    thread_mgr: RefCell<Option<ITfThreadMgr>>,
    client_id: Cell<u32>,
    thread_sink_cookie: Cell<u32>,
    attribute_atom: Cell<u32>,
    contexts: RefCell<HashMap<usize, CtxState>>,
    /// Our own interfaces, handed to TSF. Held strongly while activated (a reference cycle that
    /// `Deactivate` breaks) so that code running outside a sink callback can use them.
    composition_sink: RefCell<Option<ITfCompositionSink>>,
    edit_sink: RefCell<Option<ITfTextEditSink>>,
    /// True while one of our own edit sessions runs: edits we make must not look like the user
    /// moving the caret.
    in_our_edit: Cell<bool>,
    /// Last answer of the input-scope query per context (cheap to ask, but not for every key).
    secure_cache: RefCell<HashMap<usize, bool>>,
    pending: RefCell<Option<Pending>>,
    /// `(vk, lparam, up)` of the event whose hotkeys were already evaluated in the test half.
    fed: Cell<Option<(usize, isize, bool)>>,
}

impl Shared {
    fn new() -> Shared {
        Shared {
            thread_mgr: RefCell::new(None),
            client_id: Cell::new(0),
            thread_sink_cookie: Cell::new(0),
            attribute_atom: Cell::new(0),
            contexts: RefCell::new(HashMap::new()),
            composition_sink: RefCell::new(None),
            edit_sink: RefCell::new(None),
            in_our_edit: Cell::new(false),
            secure_cache: RefCell::new(HashMap::new()),
            pending: RefCell::new(None),
            fed: Cell::new(None),
        }
    }

    /// Engine context for `pic`, created on first sight and refreshed when the capabilities
    /// (password field) changed.
    fn context_id(&self, pic: &ITfContext) -> ContextId {
        let key = identity_of(pic);
        let cached = self.secure_cache.borrow().get(&key).copied();
        let caps = current_caps(cached.unwrap_or(false));
        let mut map = self.contexts.borrow_mut();
        if let Some(st) = map.get_mut(&key) {
            if st.caps != caps {
                st.caps = caps;
                let _ = runtime().update_context(st.id, app_identity(), caps);
            }
            return st.id;
        }
        // A new context: ask the application once whether this is a password field.
        let secure = cached.unwrap_or_else(|| self.query_secure(pic));
        let caps = current_caps(secure);
        let id = runtime().create_context(app_identity(), caps);
        let edit_cookie = self.advise_edit_sink(pic);
        trace!("context created, edit sink cookie {edit_cookie}");
        map.insert(
            key,
            CtxState {
                id,
                ctx: pic.clone(),
                composition: None,
                caps,
                edit_cookie,
            },
        );
        id
    }

    /// Listen for edits on `pic` so that the caret moving away ends the composition.
    fn advise_edit_sink(&self, pic: &ITfContext) -> u32 {
        let Some(sink) = self.edit_sink.borrow().clone() else {
            return 0;
        };
        // SAFETY: plain COM calls; the cookie is released in `unadvise_edit_sink`.
        unsafe {
            pic.cast::<ITfSource>()
                .and_then(|src| src.AdviseSink(&ITfTextEditSink::IID, &sink))
                .unwrap_or(0)
        }
    }

    fn unadvise_edit_sink(st: &CtxState) {
        if st.edit_cookie != 0 {
            // SAFETY: undoes `advise_edit_sink`.
            unsafe {
                if let Ok(src) = st.ctx.cast::<ITfSource>() {
                    let _ = src.UnadviseSink(st.edit_cookie);
                }
            }
        }
    }

    fn forget_context(&self, pic: &ITfContext) {
        self.secure_cache.borrow_mut().remove(&identity_of(pic));
        if let Some(st) = self.contexts.borrow_mut().remove(&identity_of(pic)) {
            Self::unadvise_edit_sink(&st);
            runtime().destroy_context(st.id);
        }
    }

    /// An edit happened on `pic`. When the selection moved and it was not our doing, whatever is
    /// being composed is finished: the text stays as displayed and the engine starts afresh.
    fn edit_ended(self: &Rc<Self>, pic: &ITfContext, record: &ITfEditRecord) {
        trace!("OnEndEdit in_our_edit={}", self.in_our_edit.get());
        if self.in_our_edit.get() {
            return;
        }
        // SAFETY: plain COM call on a live edit record.
        let moved = unsafe { record.GetSelectionStatus() }.is_ok_and(|b| b.as_bool());
        trace!("selection moved={moved}");
        if !moved {
            return;
        }
        let key = identity_of(pic);
        let (id, comp) = {
            let mut map = self.contexts.borrow_mut();
            let Some(st) = map.get_mut(&key) else {
                return;
            };
            let Some(comp) = st.composition.take() else {
                return;
            };
            (st.id, comp)
        };
        // The engine forgets the word right away, so no key can extend it any more.
        let _ = runtime().notify(id, Notify::CompositionTerminatedByHost);
        // Ending the composition needs a write session, which cannot run inside this callback.
        let shared = Rc::clone(self);
        let ctx = pic.clone();
        let _result = session::run_when_possible(pic, self.client_id.get(), move |ec| {
            shared.in_our_edit.set(true);
            // SAFETY: COM calls inside a valid edit session.
            let r = unsafe {
                if let Ok(range) = comp.GetRange() {
                    shared.clear_attribute(&ctx, ec, &range);
                }
                comp.EndComposition(ec)
            };
            shared.in_our_edit.set(false);
            r
        });
        trace!("end composition session -> {_result:?}");
    }

    /// Decode a key event; `None` for keys that are not ours.
    fn prepare(
        &self,
        pic: &ITfContext,
        wparam: usize,
        lparam: isize,
    ) -> Option<ime_runtime::ime_core::KeyInput> {
        let vk = wparam as u32;
        if keymap::is_foreign_key(vk) {
            return None;
        }
        let up = keymap::decode_lparam(lparam).released;
        let text = if up || is_modifier_vk(vk) {
            None
        } else {
            translate_char(vk, lparam)
        };
        let id = self.context_id(pic);
        Some(keymap::build_input(id, vk, lparam, text, read_modifiers()))
    }

    /// First half of a key event (`OnTestKey*`): evaluate hotkeys exactly once and say whether
    /// the key should be routed to the second half.
    fn test_key(self: &Rc<Self>, pic: &ITfContext, wparam: usize, lparam: isize) -> bool {
        let Some(input) = self.prepare(pic, wparam, lparam) else {
            return false;
        };
        let up = keymap::decode_lparam(lparam).released;
        if !up && !is_modifier_vk(wparam as u32) && !self.is_composing(pic) {
            self.refresh_secure(pic, input.context_id);
            self.catch_up_mode(input.context_id);
        }
        let mut rt = runtime();
        let fired: HotkeysFired = rt.feed_hotkeys(&input);
        self.fed.set(Some((wparam, lparam, up)));
        if fired.any() {
            let outcome = rt.run_hotkeys(input.context_id, fired);
            if fired.toggle {
                self.publish_mode(rt.global_mode());
            }
            let route = outcome.output.consumed || !outcome.output.actions.is_empty();
            *self.pending.borrow_mut() = Some(Pending {
                context: input.context_id,
                outcome,
            });
            return route;
        }
        !up && rt.engine_would_handle(&input)
    }

    /// Second half (`OnKey*`): act. Returns whether the key is eaten.
    fn do_key(
        self: &Rc<Self>,
        pic: &ITfContext,
        sink: &ITfCompositionSink,
        wparam: usize,
        lparam: isize,
    ) -> bool {
        let Some(input) = self.prepare(pic, wparam, lparam) else {
            return false;
        };
        let up = keymap::decode_lparam(lparam).released;
        let outcome = match self.pending.borrow_mut().take() {
            Some(p) if p.context == input.context_id => p.outcome,
            _ => {
                let mut rt = runtime();
                // Some hosts skip the test half; then the matchers have not seen this event yet.
                let already = self.fed.take() == Some((wparam, lparam, up));
                let fired = if already {
                    HotkeysFired::default()
                } else {
                    rt.feed_hotkeys(&input)
                };
                if fired.any() {
                    rt.run_hotkeys(input.context_id, fired)
                } else if up {
                    KeyOutcome::default()
                } else {
                    KeyOutcome {
                        output: rt.process_engine(input),
                        hotkey: None,
                    }
                }
            }
        };
        self.fed.set(None);
        if let Some(action) = outcome.hotkey {
            self.run_hotkey_action(action);
        }
        let eaten = outcome.output.consumed;
        self.apply(pic, sink, input.context_id, outcome.output);
        eaten
    }

    fn run_hotkey_action(&self, action: HotkeyAction) {
        match action {
            HotkeyAction::CycleScheme => {
                runtime().cycle_scheme();
            }
            // The settings app is a separate process that does not exist yet (P4).
            HotkeyAction::OpenSettings => {}
        }
    }

    /// Carry out the engine's actions in one edit session.
    fn apply(
        self: &Rc<Self>,
        pic: &ITfContext,
        sink: &ITfCompositionSink,
        id: ContextId,
        out: Output,
    ) {
        let actions: Vec<EngineAction> = out
            .actions
            .into_iter()
            .filter(|a| !matches!(a, EngineAction::ModeChanged(_)))
            .collect();
        if actions.is_empty() {
            return;
        }
        let key = identity_of(pic);
        let (shared, ctx, sink2) = (Rc::clone(self), pic.clone(), sink.clone());
        self.in_our_edit.set(true);
        let result = session::run_sync(pic, self.client_id.get(), move |ec| {
            for a in &actions {
                shared.apply_action(&ctx, ec, &sink2, key, a)?;
            }
            Ok(())
        });
        self.in_our_edit.set(false);
        if result.is_err() {
            // The document did not take our edit: forget the composition and tell the engine.
            if let Some(st) = self.contexts.borrow_mut().get_mut(&key) {
                st.composition = None;
            }
            let _ = runtime().notify(id, Notify::HostCommitFailed);
        }
    }

    fn apply_action(
        &self,
        ctx: &ITfContext,
        ec: u32,
        sink: &ITfCompositionSink,
        key: usize,
        action: &EngineAction,
    ) -> Result<()> {
        let existing = self
            .contexts
            .borrow()
            .get(&key)
            .and_then(|s| s.composition.clone());
        // SAFETY: COM calls inside a valid edit session; all pointers are live interface refs.
        unsafe {
            match action {
                EngineAction::UpdatePreedit { text, .. } => {
                    let comp = match existing {
                        Some(c) => c,
                        None => {
                            let c = start_composition(ctx, ec, sink)?;
                            self.set_composition(key, Some(c.clone()));
                            c
                        }
                    };
                    let range = comp.GetRange()?;
                    let units: Vec<u16> = text.encode_utf16().collect();
                    range.SetText(ec, 0, &units)?;
                    self.set_attribute(ctx, ec, &range)?;
                    move_caret_to_end(ctx, ec, &range)?;
                }
                EngineAction::Commit(text) => {
                    let units: Vec<u16> = text.encode_utf16().collect();
                    if let Some(comp) = existing {
                        let range = comp.GetRange()?;
                        range.SetText(ec, 0, &units)?;
                        self.clear_attribute(ctx, ec, &range);
                        move_caret_to_end(ctx, ec, &range)?;
                        self.set_composition(key, None);
                        comp.EndComposition(ec)?;
                    } else if !units.is_empty() {
                        let insert: ITfInsertAtSelection = ctx.cast()?;
                        let range = insert.InsertTextAtSelection(ec, Default::default(), &units)?;
                        move_caret_to_end(ctx, ec, &range)?;
                    }
                }
                EngineAction::CancelComposition => {
                    if let Some(comp) = existing {
                        let range = comp.GetRange()?;
                        range.SetText(ec, 0, &[])?;
                        self.clear_attribute(ctx, ec, &range);
                        self.set_composition(key, None);
                        comp.EndComposition(ec)?;
                    }
                }
                EngineAction::ModeChanged(_) => {}
            }
        }
        Ok(())
    }

    fn set_composition(&self, key: usize, c: Option<ITfComposition>) {
        if let Some(st) = self.contexts.borrow_mut().get_mut(&key) {
            st.composition = c;
        }
    }

    /// Underline the composition.
    unsafe fn set_attribute(&self, ctx: &ITfContext, ec: u32, range: &ITfRange) -> Result<()> {
        let atom = self.attribute_atom.get();
        if atom == 0 {
            return Ok(());
        }
        let prop = ctx.GetProperty(&GUID_PROP_ATTRIBUTE)?;
        // TSF expects the display attribute's guid atom as a VT_I4.
        let value = VARIANT::from(atom as i32);
        prop.SetValue(ec, range, &value)
    }

    unsafe fn clear_attribute(&self, ctx: &ITfContext, ec: u32, range: &ITfRange) {
        if let Ok(prop) = ctx.GetProperty(&GUID_PROP_ATTRIBUTE) {
            let _ = prop.Clear(ec, range);
        }
    }

    /// Composition that TSF or the application ended without us: drop engine state, emit nothing.
    fn composition_terminated(&self, which: &ITfComposition) {
        trace!("OnCompositionTerminated");
        let target = which
            .cast::<windows::core::IUnknown>()
            .map_or(0, |u| u.as_raw() as usize);
        let mut hit = None;
        for st in self.contexts.borrow_mut().values_mut() {
            let same = st
                .composition
                .as_ref()
                .and_then(|c| c.cast::<windows::core::IUnknown>().ok())
                .is_some_and(|u| u.as_raw() as usize == target);
            if same {
                st.composition = None;
                hit = Some(st.id);
            }
        }
        if let Some(id) = hit {
            let _ = runtime().notify(id, Notify::CompositionTerminatedByHost);
        }
    }

    /// The shared Vietnamese/English state lives in a global TSF compartment so that every
    /// process sees the same value (spec §15.2).
    fn compartment(&self) -> Option<ITfCompartment> {
        let tm = self.thread_mgr.borrow().clone()?;
        // SAFETY: plain COM calls on a live thread manager.
        unsafe {
            tm.GetGlobalCompartment()
                .ok()?
                .GetCompartment(&GUID_COMPARTMENT_MODE)
                .ok()
        }
    }

    fn publish_mode(&self, mode: InputMode) {
        let Some(c) = self.compartment() else {
            return;
        };
        let value = VARIANT::from(i32::from(mode == InputMode::English));
        // SAFETY: plain COM call.
        let _r = unsafe { c.SetValue(self.client_id.get(), &value) };
        trace!("publish mode {mode:?} -> {_r:?}");
    }

    /// The mode other processes published, if any.
    fn read_published_mode(&self) -> Option<InputMode> {
        let c = self.compartment()?;
        // SAFETY: plain COM call; the VARIANT is read as an i32 only when it is one.
        let v = unsafe { c.GetValue() }.ok()?;
        let n = i32::try_from(&v).ok()?;
        Some(if n == 1 {
            InputMode::English
        } else {
            InputMode::Vietnamese
        })
    }

    /// Ask the application whether this field is a password / private input (TSF input scope)
    /// and remember the answer.
    fn query_secure(&self, pic: &ITfContext) -> bool {
        let key = identity_of(pic);
        let found = Rc::new(Cell::new(false));
        let out = Rc::clone(&found);
        let ctx = pic.clone();
        let r = session::read_sync(pic, self.client_id.get(), move |ec| {
            // SAFETY: COM calls inside a valid read-only edit session.
            let r = unsafe { input_scope_is_secure(&ctx, ec) };
            trace!("input_scope_is_secure -> {r:?}");
            out.set(r.unwrap_or(false));
            Ok(())
        });
        trace!("query_secure session -> {r:?}");
        // If the document cannot be read right now, keep the last known answer.
        let secure = if r.is_ok() {
            found.get()
        } else {
            self.secure_cache
                .borrow()
                .get(&key)
                .copied()
                .unwrap_or(false)
        };
        self.secure_cache.borrow_mut().insert(key, secure);
        secure
    }

    /// At the start of a word the field may have changed (tabbing from a user name to a
    /// password box keeps the same TSF context in many apps): ask again and tell the engine.
    fn refresh_secure(&self, pic: &ITfContext, id: ContextId) {
        let key = identity_of(pic);
        let before = self
            .secure_cache
            .borrow()
            .get(&key)
            .copied()
            .unwrap_or(false);
        let now = self.query_secure(pic);
        if now == before {
            return;
        }
        let caps = current_caps(now);
        if let Some(st) = self.contexts.borrow_mut().get_mut(&key) {
            st.caps = caps;
        }
        let _ = runtime().update_context(id, app_identity(), caps);
    }

    fn is_composing(&self, pic: &ITfContext) -> bool {
        self.contexts
            .borrow()
            .get(&identity_of(pic))
            .is_some_and(|s| s.composition.is_some())
    }

    /// At the start of a word, adopt a Vietnamese/English change another application published.
    /// This duplicates what `OnSetFocus` does; some hosts never deliver that notification, and a
    /// toggle must not be missed because of it.
    fn catch_up_mode(&self, id: ContextId) {
        let published = self.read_published_mode();
        let mut rt = runtime();
        if let Some(m) = published {
            rt.set_global_mode(m);
        }
        let _ = rt.sync_mode(id);
    }

    /// Focus moved to another document: whatever was being composed stays as displayed.
    fn focus_changed(&self, previous: Option<&ITfDocumentMgr>, current: Option<&ITfDocumentMgr>) {
        if let Some(prev) = previous {
            // SAFETY: plain COM call on a live document manager.
            if let Ok(top) = unsafe { prev.GetTop() } {
                let key = identity_of(&top);
                if let Some(st) = self.contexts.borrow_mut().get_mut(&key) {
                    st.composition = None;
                    let _ = runtime().notify(st.id, Notify::CompositionTerminatedByHost);
                }
            }
        }
        let published = self.read_published_mode();
        // SAFETY: plain COM call on a live document manager.
        let top = current.and_then(|d| unsafe { d.GetTop() }.ok());
        let id = top.as_ref().map(|c| self.context_id(c));
        let mut rt = runtime();
        rt.reset_hotkeys();
        rt.poll_reload();
        if let Some(m) = published {
            rt.set_global_mode(m);
        }
        if let Some(id) = id {
            let _ = rt.sync_mode(id);
        }
    }
}

/// Is the field's input scope a password or private one?
unsafe fn input_scope_is_secure(ctx: &ITfContext, ec: u32) -> Result<bool> {
    let range = ctx.GetStart(ec)?;
    // An application-defined property, so `GetAppProperty` (not `GetProperty`).
    let prop = ctx.GetAppProperty(&GUID_PROP_INPUTSCOPE)?;
    let value = prop.GetValue(ec, &range);
    trace!(
        "input scope GetValue -> {:?}",
        value.as_ref().map(|v| v.is_empty())
    );
    let value = value?;
    let Ok(unknown) = windows::core::IUnknown::try_from(&value) else {
        return Ok(false); // no scope set
    };
    let scope: ITfInputScope = unknown.cast()?;
    let mut scopes: *mut InputScope = std::ptr::null_mut();
    let mut count = 0u32;
    scope.GetInputScopes(&mut scopes, &mut count)?;
    if scopes.is_null() {
        return Ok(false);
    }
    let list = std::slice::from_raw_parts(scopes, count as usize);
    let secure = list.iter().any(|s| *s == IS_PASSWORD || *s == IS_PRIVATE);
    // The array is allocated by the callee with CoTaskMemAlloc.
    windows::Win32::System::Com::CoTaskMemFree(Some(scopes.cast()));
    Ok(secure)
}

unsafe fn start_composition(
    ctx: &ITfContext,
    ec: u32,
    sink: &ITfCompositionSink,
) -> Result<ITfComposition> {
    let insert: ITfInsertAtSelection = ctx.cast()?;
    // Query only: the (empty or selected) range at the caret, without inserting anything.
    let range = insert.InsertTextAtSelection(ec, TF_IAS_QUERYONLY, &[])?;
    let cc: ITfContextComposition = ctx.cast()?;
    cc.StartComposition(ec, &range, sink)
}

unsafe fn move_caret_to_end(ctx: &ITfContext, ec: u32, range: &ITfRange) -> Result<()> {
    let caret = range.Clone()?;
    caret.Collapse(ec, TF_ANCHOR_END)?;
    let selection = TF_SELECTION {
        range: ManuallyDrop::new(Some(caret)),
        style: TF_SELECTIONSTYLE {
            ase: TF_AE_NONE,
            fInterimChar: FALSE,
        },
    };
    ctx.SetSelection(ec, &[selection])
}

#[implement(
    ITfTextInputProcessorEx,
    ITfThreadMgrEventSink,
    ITfKeyEventSink,
    ITfCompositionSink,
    ITfTextEditSink,
    ITfDisplayAttributeProvider
)]
pub struct TextService {
    shared: Rc<Shared>,
}

impl TextService {
    pub fn new() -> TextService {
        LIVE_OBJECTS.fetch_add(1, Ordering::Relaxed);
        TextService {
            shared: Rc::new(Shared::new()),
        }
    }
}

impl Drop for TextService {
    fn drop(&mut self) {
        LIVE_OBJECTS.fetch_sub(1, Ordering::Relaxed);
    }
}

impl TextService_Impl {
    fn activate(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32) -> Result<()> {
        let tm: ITfThreadMgr = ptim.ok()?.clone();
        let shared = &self.shared;
        shared.client_id.set(tid);
        // SAFETY: COM calls with live interface pointers; sinks are unadvised in `Deactivate`.
        unsafe {
            let source: ITfSource = tm.cast()?;
            let thread_sink: ITfThreadMgrEventSink = self.to_interface();
            let cookie = source.AdviseSink(&ITfThreadMgrEventSink::IID, &thread_sink)?;
            shared.thread_sink_cookie.set(cookie);

            let keystroke: ITfKeystrokeMgr = tm.cast()?;
            *shared.composition_sink.borrow_mut() = Some(self.to_interface());
            *shared.edit_sink.borrow_mut() = Some(self.to_interface());
            let key_sink: ITfKeyEventSink = self.to_interface();
            let advised = keystroke.AdviseKeyEventSink(tid, &key_sink, true);
            #[cfg(not(feature = "test-hooks"))]
            advised?;
            #[cfg(feature = "test-hooks")]
            let _ = advised;

            let cat: ITfCategoryMgr =
                CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)?;
            shared
                .attribute_atom
                .set(cat.RegisterGUID(&GUID_DISPLAY_ATTRIBUTE_COMPOSITION)?);
        }
        *shared.thread_mgr.borrow_mut() = Some(tm);
        Ok(())
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32) -> Result<()> {
        guard(fail(), || self.activate(ptim, tid))
    }

    fn Deactivate(&self) -> Result<()> {
        guard(fail(), || {
            let shared = &self.shared;
            if let Some(tm) = shared.thread_mgr.borrow_mut().take() {
                // SAFETY: undoes exactly what `activate` advised.
                unsafe {
                    if let Ok(k) = tm.cast::<ITfKeystrokeMgr>() {
                        let _ = k.UnadviseKeyEventSink(shared.client_id.get());
                    }
                    if let Ok(s) = tm.cast::<ITfSource>() {
                        let _ = s.UnadviseSink(shared.thread_sink_cookie.get());
                    }
                }
            }
            let contexts: Vec<CtxState> = shared
                .contexts
                .borrow_mut()
                .drain()
                .map(|(_, v)| v)
                .collect();
            for st in contexts {
                runtime().destroy_context(st.id);
            }
            shared.client_id.set(0);
            Ok(())
        })
    }
}

impl ITfTextInputProcessorEx_Impl for TextService_Impl {
    fn ActivateEx(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32, _flags: u32) -> Result<()> {
        guard(fail(), || self.activate(ptim, tid))
    }
}

impl ITfThreadMgrEventSink_Impl for TextService_Impl {
    fn OnInitDocumentMgr(&self, _pdim: Ref<'_, ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }

    fn OnUninitDocumentMgr(&self, pdim: Ref<'_, ITfDocumentMgr>) -> Result<()> {
        guard(Ok(()), || {
            if let Ok(dim) = pdim.ok() {
                // SAFETY: plain COM call.
                if let Ok(top) = unsafe { dim.GetTop() } {
                    self.shared.forget_context(&top);
                }
            }
            Ok(())
        })
    }

    fn OnSetFocus(
        &self,
        pdimfocus: Ref<'_, ITfDocumentMgr>,
        pdimprevfocus: Ref<'_, ITfDocumentMgr>,
    ) -> Result<()> {
        guard(Ok(()), || {
            self.shared
                .focus_changed(pdimprevfocus.as_ref(), pdimfocus.as_ref());
            Ok(())
        })
    }

    fn OnPushContext(&self, _pic: Ref<'_, ITfContext>) -> Result<()> {
        Ok(())
    }

    fn OnPopContext(&self, pic: Ref<'_, ITfContext>) -> Result<()> {
        guard(Ok(()), || {
            if let Ok(c) = pic.ok() {
                self.shared.forget_context(c);
            }
            Ok(())
        })
    }
}

impl ITfKeyEventSink_Impl for TextService_Impl {
    fn OnSetFocus(&self, _foreground: BOOL) -> Result<()> {
        Ok(())
    }

    fn OnTestKeyDown(
        &self,
        pic: Ref<'_, ITfContext>,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<BOOL> {
        guard(Ok(FALSE), || {
            let pic = pic.ok().map_err(|_| E_POINTER)?;
            Ok(self.shared.test_key(pic, wparam.0, lparam.0).into())
        })
    }

    fn OnTestKeyUp(
        &self,
        pic: Ref<'_, ITfContext>,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<BOOL> {
        guard(Ok(FALSE), || {
            let pic = pic.ok().map_err(|_| E_POINTER)?;
            Ok(self.shared.test_key(pic, wparam.0, lparam.0).into())
        })
    }

    fn OnKeyDown(&self, pic: Ref<'_, ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        guard(Ok(FALSE), || {
            let pic = pic.ok().map_err(|_| E_POINTER)?;
            let sink: ITfCompositionSink = self.to_interface();
            Ok(self.shared.do_key(pic, &sink, wparam.0, lparam.0).into())
        })
    }

    fn OnKeyUp(&self, pic: Ref<'_, ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        guard(Ok(FALSE), || {
            let pic = pic.ok().map_err(|_| E_POINTER)?;
            let sink: ITfCompositionSink = self.to_interface();
            Ok(self.shared.do_key(pic, &sink, wparam.0, lparam.0).into())
        })
    }

    fn OnPreservedKey(&self, _pic: Ref<'_, ITfContext>, _rguid: *const GUID) -> Result<BOOL> {
        Ok(FALSE)
    }
}

impl ITfTextEditSink_Impl for TextService_Impl {
    fn OnEndEdit(
        &self,
        pic: Ref<'_, ITfContext>,
        _ecreadonly: u32,
        peditrecord: Ref<'_, ITfEditRecord>,
    ) -> Result<()> {
        guard(Ok(()), || {
            if let (Ok(pic), Ok(record)) = (pic.ok(), peditrecord.ok()) {
                self.shared.edit_ended(pic, record);
            }
            Ok(())
        })
    }
}

impl ITfCompositionSink_Impl for TextService_Impl {
    fn OnCompositionTerminated(
        &self,
        _ecwrite: u32,
        pcomposition: Ref<'_, ITfComposition>,
    ) -> Result<()> {
        guard(Ok(()), || {
            if let Ok(c) = pcomposition.ok() {
                self.shared.composition_terminated(c);
            }
            Ok(())
        })
    }
}

impl ITfDisplayAttributeProvider_Impl for TextService_Impl {
    fn EnumDisplayAttributeInfo(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        Ok(display::enum_info())
    }

    fn GetDisplayAttributeInfo(&self, guid: *const GUID) -> Result<ITfDisplayAttributeInfo> {
        if guid.is_null() {
            return Err(E_INVALIDARG.into());
        }
        // SAFETY: checked for NULL.
        display::info_for(unsafe { &*guid })
    }
}
