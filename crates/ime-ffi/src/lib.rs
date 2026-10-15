//! C ABI over [`ime_runtime::Runtime`] (spec §13); the C declarations are in `include/pkey.h`.
//!
//! Rules every function follows:
//! - No panic crosses the boundary: each body runs under `catch_unwind`. A panic returns
//!   `VT_ERR_PANIC` and *poisons* the engine, after which keys are reported as not consumed
//!   (fail open) until `vt_engine_free`.
//! - Pointers handed out (`VtEngine*`, `VtResult*`) are `Box`es released only by the matching
//!   `*_free`. Text pointers inside a `VtActionView` borrow from their `VtResult`.
//! - Input pointers may be NULL only where the header says so; lengths are checked with the
//!   pointer, and text must be UTF-8.
//! - One engine is used by one thread at a time (not enforced here).

use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};

use ime_runtime::ime_config::{AppIdentity, Config};
use ime_runtime::ime_core::{
    Capabilities, ContextId, EngineAction, InputMode, KeyFlags, KeyInput, LogicalKey, Modifiers,
    Notify, Output,
};
use ime_runtime::{HotkeyAction, Runtime};

pub const VT_ABI_VERSION: u32 = 1;

pub const VT_OK: u32 = 0;
pub const VT_ERR_NULL: u32 = 1;
pub const VT_ERR_INVALID_UTF8: u32 = 2;
pub const VT_ERR_CONFIG: u32 = 3;
pub const VT_ERR_UNKNOWN_CONTEXT: u32 = 4;
pub const VT_ERR_ABI_MISMATCH: u32 = 5;
pub const VT_ERR_INVALID_ARG: u32 = 6;
pub const VT_ERR_PANIC: u32 = 255;

/// `VtStatus` as it crosses the boundary (a C enum is an `int`).
pub type VtStatus = u32;
pub type VtContextId = u64;

const ACTION_UPDATE_PREEDIT: u32 = 1;
const ACTION_COMMIT: u32 = 2;
const ACTION_CANCEL: u32 = 3;
const ACTION_MODE_CHANGED: u32 = 4;

pub struct VtEngine {
    rt: Runtime,
    poisoned: AtomicBool,
}

struct Action {
    kind: u32,
    text: String,
    caret: [u32; 4], // utf16, scalars, graphemes, utf8
    mode: u32,
}

pub struct VtResult {
    consumed: bool,
    hotkey: u32,
    actions: Vec<Action>,
}

#[repr(C)]
pub struct VtKeyEvent {
    pub struct_size: u32,
    /// `VtKey` (validated; an out-of-range value from C must not become a Rust enum).
    pub key: u32,
    pub ch: u32,
    pub physical_code: u32,
    pub modifiers: u16,
    pub flags: u8,
}

#[repr(C)]
pub struct VtActionView {
    pub kind: u32,
    pub text_utf8: *const u8,
    pub text_len: usize,
    pub caret_utf16: u32,
    pub caret_scalars: u32,
    pub caret_graphemes: u32,
    pub caret_utf8: u32,
    pub mode: u32,
}

#[repr(C)]
pub struct VtContextInfo {
    pub struct_size: u32,
    pub app_id_utf8: *const u8,
    pub app_id_len: usize,
    pub app_id_kind: u32,
    pub capabilities: u32,
}

// ---------------------------------------------------------------------- helpers

/// `Err(status)` for the early-return style used below.
type Res<T> = Result<T, VtStatus>;

/// Bytes at `ptr`/`len` as UTF-8. NULL with length 0 is the empty string.
unsafe fn utf8<'a>(ptr: *const u8, len: usize) -> Res<&'a str> {
    if ptr.is_null() {
        return if len == 0 { Ok("") } else { Err(VT_ERR_NULL) };
    }
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).map_err(|_| VT_ERR_INVALID_UTF8)
}

unsafe fn engine_mut<'a>(e: *mut VtEngine) -> Res<&'a mut VtEngine> {
    e.as_mut().ok_or(VT_ERR_NULL)
}

fn app_and_caps(info: *const VtContextInfo) -> Res<(AppIdentity, Capabilities)> {
    // SAFETY: the caller promises `info` is NULL or points to a readable VtContextInfo.
    let Some(info) = (unsafe { info.as_ref() }) else {
        return Ok((AppIdentity::default(), Capabilities::empty()));
    };
    if (info.struct_size as usize) < std::mem::size_of::<VtContextInfo>() {
        return Err(VT_ERR_ABI_MISMATCH);
    }
    // SAFETY: pointer/length come from the caller's struct, checked for NULL inside.
    let id = unsafe { utf8(info.app_id_utf8, info.app_id_len)? };
    let mut app = AppIdentity::default();
    if !id.is_empty() {
        let own = Some(id.to_string());
        match info.app_id_kind {
            1 => {
                if id.contains(['\\', '/']) {
                    app.exe_name = id.rsplit(['\\', '/']).next().map(str::to_string);
                    app.process_path = own;
                } else {
                    app.exe_name = own;
                }
            }
            2 => app.bundle_id = own,
            3 => app.program = own,
            4 => app.ibus_client = own,
            5 => app.aumid = own,
            _ => {}
        }
    }
    Ok((app, Capabilities::from_bits_truncate(info.capabilities)))
}

fn key_from_c(ctx: ContextId, ev: *const VtKeyEvent) -> Res<KeyInput> {
    // SAFETY: the caller promises `ev` is NULL or points to a readable VtKeyEvent.
    let ev = unsafe { ev.as_ref() }.ok_or(VT_ERR_NULL)?;
    if (ev.struct_size as usize) < std::mem::size_of::<VtKeyEvent>() {
        return Err(VT_ERR_ABI_MISMATCH);
    }
    let logical_key = match ev.key {
        0 => LogicalKey::Char,
        1 => LogicalKey::Backspace,
        2 => LogicalKey::Delete,
        3 => LogicalKey::Enter,
        4 => LogicalKey::Tab,
        5 => LogicalKey::Escape,
        6 => LogicalKey::Space,
        7 => LogicalKey::Left,
        8 => LogicalKey::Right,
        9 => LogicalKey::Up,
        10 => LogicalKey::Down,
        11 => LogicalKey::Home,
        12 => LogicalKey::End,
        13 => LogicalKey::PageUp,
        14 => LogicalKey::PageDown,
        15 => LogicalKey::Other,
        _ => return Err(VT_ERR_INVALID_ARG),
    };
    let text = match ev.ch {
        0 => None,
        c => Some(char::from_u32(c).ok_or(VT_ERR_INVALID_ARG)?),
    };
    Ok(KeyInput {
        context_id: ctx,
        logical_key,
        text,
        physical_code: ev.physical_code,
        modifiers: Modifiers::from_bits_truncate(ev.modifiers),
        flags: KeyFlags::from_bits_truncate(ev.flags),
    })
}

fn to_result(out: Output, hotkey: Option<HotkeyAction>) -> *mut VtResult {
    let actions = out
        .actions
        .into_iter()
        .map(|a| match a {
            EngineAction::UpdatePreedit { text, caret } => Action {
                kind: ACTION_UPDATE_PREEDIT,
                text,
                caret: [caret.utf16, caret.scalars, caret.graphemes, caret.utf8],
                mode: 0,
            },
            EngineAction::Commit(text) => Action {
                kind: ACTION_COMMIT,
                text,
                caret: [0; 4],
                mode: 0,
            },
            EngineAction::CancelComposition => Action {
                kind: ACTION_CANCEL,
                text: String::new(),
                caret: [0; 4],
                mode: 0,
            },
            EngineAction::ModeChanged(m) => Action {
                kind: ACTION_MODE_CHANGED,
                text: String::new(),
                caret: [0; 4],
                mode: u32::from(m == InputMode::English),
            },
        })
        .collect();
    Box::into_raw(Box::new(VtResult {
        consumed: out.consumed,
        hotkey: match hotkey {
            None => 0,
            Some(HotkeyAction::CycleScheme) => 1,
            Some(HotkeyAction::OpenSettings) => 2,
        },
        actions,
    }))
}

fn empty_result() -> *mut VtResult {
    to_result(Output::default(), None)
}

/// Run `f`; a panic becomes `VT_ERR_PANIC` and sets the engine's `poison` flag (if given).
fn guarded(poison: Option<&AtomicBool>, f: impl FnOnce() -> VtStatus) -> VtStatus {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(s) => s,
        Err(_) => {
            if let Some(p) = poison {
                p.store(true, Ordering::Relaxed);
            }
            VT_ERR_PANIC
        }
    }
}

fn new_engine(rt: Runtime) -> *mut VtEngine {
    Box::into_raw(Box::new(VtEngine {
        rt,
        poisoned: AtomicBool::new(false),
    }))
}

// ---------------------------------------------------------------------- engine

#[no_mangle]
pub extern "C" fn vt_abi_version() -> u32 {
    VT_ABI_VERSION
}

/// # Safety
/// `config_toml` must be NULL (with `len == 0`) or point to `len` readable bytes; `out` must be
/// writable.
#[no_mangle]
pub unsafe extern "C" fn vt_engine_new(
    config_toml: *const u8,
    len: usize,
    out: *mut *mut VtEngine,
) -> VtStatus {
    if out.is_null() {
        return VT_ERR_NULL;
    }
    *out = std::ptr::null_mut();
    guarded(None, || {
        let text = match utf8(config_toml, len) {
            Ok(t) => t,
            Err(s) => return s,
        };
        match Config::parse(text) {
            Ok(l) => {
                *out = new_engine(Runtime::new(l.config, Vec::new()));
                VT_OK
            }
            Err(_) => VT_ERR_CONFIG,
        }
    })
}

/// # Safety
/// `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn vt_engine_new_from_default_path(out: *mut *mut VtEngine) -> VtStatus {
    if out.is_null() {
        return VT_ERR_NULL;
    }
    *out = std::ptr::null_mut();
    guarded(None, || {
        *out = new_engine(Runtime::from_default_paths());
        VT_OK
    })
}

/// # Safety
/// `e` must be NULL or a pointer from `vt_engine_new*` that has not been freed.
#[no_mangle]
pub unsafe extern "C" fn vt_engine_free(e: *mut VtEngine) {
    if !e.is_null() {
        // A destructor panic must not unwind into C.
        let _ = catch_unwind(AssertUnwindSafe(|| drop(Box::from_raw(e))));
    }
}

/// # Safety
/// `e` must be a live engine; `toml` must point to `len` readable bytes (or be NULL with 0).
#[no_mangle]
pub unsafe extern "C" fn vt_engine_reload_config(
    e: *mut VtEngine,
    toml: *const u8,
    len: usize,
) -> VtStatus {
    let eng = match engine_mut(e) {
        Ok(e) => e,
        Err(s) => return s,
    };
    guarded(Some(&eng.poisoned), || {
        let text = match utf8(toml, len) {
            Ok(t) => t,
            Err(s) => return s,
        };
        match Config::parse(text) {
            Ok(l) => {
                eng.rt.apply(l.config, Vec::new());
                VT_OK
            }
            Err(_) => VT_ERR_CONFIG,
        }
    })
}

/// # Safety
/// `e` must be a live engine.
#[no_mangle]
pub unsafe extern "C" fn vt_engine_poll_reload(e: *mut VtEngine) -> c_int {
    let Ok(eng) = engine_mut(e) else {
        return 0;
    };
    if eng.poisoned.load(Ordering::Relaxed) {
        return 0;
    }
    let changed = catch_unwind(AssertUnwindSafe(|| eng.rt.poll_reload()));
    match changed {
        Ok(c) => c_int::from(c),
        Err(_) => {
            eng.poisoned.store(true, Ordering::Relaxed);
            0
        }
    }
}

// ---------------------------------------------------------------------- contexts

/// # Safety
/// `e` must be a live engine, `info` NULL or readable, `out` writable.
#[no_mangle]
pub unsafe extern "C" fn vt_context_create(
    e: *mut VtEngine,
    info: *const VtContextInfo,
    out: *mut VtContextId,
) -> VtStatus {
    let eng = match engine_mut(e) {
        Ok(e) => e,
        Err(s) => return s,
    };
    if out.is_null() {
        return VT_ERR_NULL;
    }
    guarded(Some(&eng.poisoned), || {
        let (app, caps) = match app_and_caps(info) {
            Ok(v) => v,
            Err(s) => return s,
        };
        *out = eng.rt.create_context(app, caps);
        VT_OK
    })
}

/// # Safety
/// `e` must be NULL or a live engine.
#[no_mangle]
pub unsafe extern "C" fn vt_context_destroy(e: *mut VtEngine, id: VtContextId) {
    if let Ok(eng) = engine_mut(e) {
        let _ = catch_unwind(AssertUnwindSafe(|| eng.rt.destroy_context(id)));
    }
}

/// # Safety
/// `e` must be a live engine, `info` NULL or readable, `out` writable.
#[no_mangle]
pub unsafe extern "C" fn vt_context_update(
    e: *mut VtEngine,
    id: VtContextId,
    info: *const VtContextInfo,
    out: *mut *mut VtResult,
) -> VtStatus {
    with_result(e, id, out, |eng| {
        let (app, caps) = app_and_caps(info)?;
        let o = eng.rt.update_context(id, app, caps);
        Ok(to_result(o, None))
    })
}

/// Common shape of the functions that return a `VtResult`.
unsafe fn with_result(
    e: *mut VtEngine,
    id: VtContextId,
    out: *mut *mut VtResult,
    f: impl FnOnce(&mut VtEngine) -> Res<*mut VtResult>,
) -> VtStatus {
    if out.is_null() {
        return VT_ERR_NULL;
    }
    *out = std::ptr::null_mut();
    let eng = match engine_mut(e) {
        Ok(e) => e,
        Err(s) => return s,
    };
    if !eng.rt.engine().has_context(id) {
        return VT_ERR_UNKNOWN_CONTEXT;
    }
    if eng.poisoned.load(Ordering::Relaxed) {
        *out = empty_result();
        return VT_OK;
    }
    let (poison, rt_holder) = (&eng.poisoned as *const AtomicBool, eng as *mut VtEngine);
    // SAFETY: `poison` and `rt_holder` point into the same live engine; the closure only touches
    // the runtime, never the flag, so the two accesses do not overlap.
    guarded(Some(&*poison), || match f(&mut *rt_holder) {
        Ok(r) => {
            *out = r;
            VT_OK
        }
        Err(s) => s,
    })
}

// ---------------------------------------------------------------------- keys

/// # Safety
/// `e` must be NULL or a live engine; `ev` NULL or readable.
#[no_mangle]
pub unsafe extern "C" fn vt_would_consume(
    e: *const VtEngine,
    id: VtContextId,
    ev: *const VtKeyEvent,
) -> c_int {
    let Some(eng) = e.as_ref() else {
        return 0;
    };
    if eng.poisoned.load(Ordering::Relaxed) {
        return 0;
    }
    let Ok(key) = key_from_c(id, ev) else {
        return 0;
    };
    match catch_unwind(AssertUnwindSafe(|| eng.rt.would_consume(&key))) {
        Ok(c) => c_int::from(c),
        Err(_) => {
            eng.poisoned.store(true, Ordering::Relaxed);
            0
        }
    }
}

/// # Safety
/// `e` must be a live engine; `ev` readable; `out` writable. Free the result with
/// `vt_result_free`.
#[no_mangle]
pub unsafe extern "C" fn vt_process_key(
    e: *mut VtEngine,
    id: VtContextId,
    ev: *const VtKeyEvent,
    out: *mut *mut VtResult,
) -> VtStatus {
    with_result(e, id, out, |eng| {
        let key = key_from_c(id, ev)?;
        let o = eng.rt.handle_key(key);
        Ok(to_result(o.output, o.hotkey))
    })
}

/// # Safety
/// As `vt_process_key`.
#[no_mangle]
pub unsafe extern "C" fn vt_notify(
    e: *mut VtEngine,
    id: VtContextId,
    notify_kind: u32,
    out: *mut *mut VtResult,
) -> VtStatus {
    with_result(e, id, out, |eng| {
        let n = match notify_kind {
            1 => Notify::FocusOut,
            2 => Notify::CaretMoved,
            3 => Notify::SelectionChanged,
            4 => Notify::CompositionTerminatedByHost,
            5 => Notify::HostCommitFailed,
            6 => Notify::Reset,
            _ => return Err(VT_ERR_INVALID_ARG),
        };
        if matches!(n, Notify::FocusOut | Notify::Reset) {
            // Key-up events may have been lost while another window had focus.
            eng.rt.reset_hotkeys();
        }
        Ok(to_result(eng.rt.notify(id, n), None))
    })
}

/// # Safety
/// As `vt_process_key`.
#[no_mangle]
pub unsafe extern "C" fn vt_set_mode(
    e: *mut VtEngine,
    id: VtContextId,
    mode: u32,
    out: *mut *mut VtResult,
) -> VtStatus {
    with_result(e, id, out, |eng| {
        let m = match mode {
            0 => InputMode::Vietnamese,
            1 => InputMode::English,
            _ => return Err(VT_ERR_INVALID_ARG),
        };
        Ok(to_result(eng.rt.set_mode(id, m), None))
    })
}

/// # Safety
/// `e` must be NULL or a live engine.
#[no_mangle]
pub unsafe extern "C" fn vt_get_mode(e: *const VtEngine, id: VtContextId) -> c_int {
    let Some(eng) = e.as_ref() else {
        return -1;
    };
    if !eng.rt.engine().has_context(id) {
        return -1;
    }
    match eng.rt.mode(id) {
        InputMode::Vietnamese => 0,
        InputMode::English => 1,
    }
}

// ---------------------------------------------------------------------- results

/// # Safety
/// `r` must be NULL or a live result.
#[no_mangle]
pub unsafe extern "C" fn vt_result_consumed(r: *const VtResult) -> c_int {
    r.as_ref().map_or(0, |r| c_int::from(r.consumed))
}

/// # Safety
/// `r` must be NULL or a live result.
#[no_mangle]
pub unsafe extern "C" fn vt_result_hotkey(r: *const VtResult) -> u32 {
    r.as_ref().map_or(0, |r| r.hotkey)
}

/// # Safety
/// `r` must be NULL or a live result.
#[no_mangle]
pub unsafe extern "C" fn vt_result_len(r: *const VtResult) -> usize {
    r.as_ref().map_or(0, |r| r.actions.len())
}

/// # Safety
/// `r` must be NULL or a live result; `out` writable. The text pointer written to `out` is valid
/// until `vt_result_free(r)`.
#[no_mangle]
pub unsafe extern "C" fn vt_result_get(
    r: *const VtResult,
    i: usize,
    out: *mut VtActionView,
) -> VtStatus {
    let Some(r) = r.as_ref() else {
        return VT_ERR_NULL;
    };
    if out.is_null() {
        return VT_ERR_NULL;
    }
    let Some(a) = r.actions.get(i) else {
        return VT_ERR_INVALID_ARG;
    };
    let has_text = matches!(a.kind, ACTION_UPDATE_PREEDIT | ACTION_COMMIT);
    *out = VtActionView {
        kind: a.kind,
        text_utf8: if has_text {
            a.text.as_ptr()
        } else {
            std::ptr::null()
        },
        text_len: if has_text { a.text.len() } else { 0 },
        caret_utf16: a.caret[0],
        caret_scalars: a.caret[1],
        caret_graphemes: a.caret[2],
        caret_utf8: a.caret[3],
        mode: a.mode,
    };
    VT_OK
}

/// # Safety
/// `r` must be NULL or a result that has not been freed.
#[no_mangle]
pub unsafe extern "C" fn vt_result_free(r: *mut VtResult) {
    if !r.is_null() {
        drop(Box::from_raw(r));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::{null, null_mut};

    struct Eng(*mut VtEngine);
    impl Drop for Eng {
        fn drop(&mut self) {
            unsafe { vt_engine_free(self.0) }
        }
    }

    fn engine(toml: &str) -> Eng {
        let mut e = null_mut();
        let s = unsafe { vt_engine_new(toml.as_ptr(), toml.len(), &mut e) };
        assert_eq!(s, VT_OK);
        assert!(!e.is_null());
        Eng(e)
    }

    fn context(e: &Eng, app: &str, kind: u32, caps: u32) -> u64 {
        let info = VtContextInfo {
            struct_size: std::mem::size_of::<VtContextInfo>() as u32,
            app_id_utf8: app.as_ptr(),
            app_id_len: app.len(),
            app_id_kind: kind,
            capabilities: caps,
        };
        let mut id = 0;
        assert_eq!(unsafe { vt_context_create(e.0, &info, &mut id) }, VT_OK);
        id
    }

    fn ev(key: u32, ch: char) -> VtKeyEvent {
        VtKeyEvent {
            struct_size: std::mem::size_of::<VtKeyEvent>() as u32,
            key,
            ch: if ch == '\0' { 0 } else { ch as u32 },
            physical_code: 0,
            modifiers: 0,
            flags: 0,
        }
    }

    /// (consumed, [(kind, text)])
    fn collect(r: *mut VtResult) -> (bool, Vec<(u32, String)>) {
        assert!(!r.is_null());
        unsafe {
            let consumed = vt_result_consumed(r) != 0;
            let mut v = Vec::new();
            for i in 0..vt_result_len(r) {
                let mut a = std::mem::zeroed::<VtActionView>();
                assert_eq!(vt_result_get(r, i, &mut a), VT_OK);
                let text = if a.text_utf8.is_null() {
                    String::new()
                } else {
                    String::from_utf8(std::slice::from_raw_parts(a.text_utf8, a.text_len).to_vec())
                        .unwrap()
                };
                v.push((a.kind, text));
            }
            vt_result_free(r);
            (consumed, v)
        }
    }

    fn press(e: &Eng, id: u64, key: u32, ch: char) -> (bool, Vec<(u32, String)>) {
        let k = ev(key, ch);
        let mut r = null_mut();
        assert_eq!(unsafe { vt_process_key(e.0, id, &k, &mut r) }, VT_OK);
        collect(r)
    }

    fn type_str(e: &Eng, id: u64, s: &str) -> Vec<(u32, String)> {
        let mut all = Vec::new();
        for c in s.chars() {
            let key = if c == ' ' { 6 } else { 0 };
            all.extend(press(e, id, key, c).1);
        }
        all
    }

    #[test]
    fn version() {
        assert_eq!(vt_abi_version(), VT_ABI_VERSION);
    }

    #[test]
    fn types_vietnamese_through_the_c_api() {
        let e = engine("");
        let id = context(&e, "", 0, 1);
        let actions = type_str(&e, id, "tieengs ");
        assert_eq!(actions.last(), Some(&(ACTION_COMMIT, "tiếng".to_string())));
        let (consumed, a) = press(&e, id, 0, 'a');
        assert!(consumed);
        assert_eq!(a, [(ACTION_UPDATE_PREEDIT, "a".to_string())]);
    }

    #[test]
    fn preedit_carries_every_caret_unit() {
        let e = engine("");
        let id = context(&e, "", 0, 1);
        let mut r: *mut VtResult = null_mut();
        unsafe {
            for c in "tieeng".chars() {
                if !r.is_null() {
                    vt_result_free(r);
                }
                assert_eq!(vt_process_key(e.0, id, &ev(0, c), &mut r), VT_OK);
            }
            let mut a = std::mem::zeroed::<VtActionView>();
            assert_eq!(vt_result_get(r, 0, &mut a), VT_OK);
            assert_eq!(a.kind, ACTION_UPDATE_PREEDIT);
            // "tiêng": 5 letters; ê is 2 bytes in UTF-8 and one UTF-16 unit.
            assert_eq!(
                (
                    a.caret_graphemes,
                    a.caret_scalars,
                    a.caret_utf16,
                    a.caret_utf8
                ),
                (5, 5, 5, 6)
            );
            vt_result_free(r);
        }
    }

    #[test]
    fn would_consume_does_not_change_state() {
        let e = engine("");
        let id = context(&e, "", 0, 1);
        for c in "tieengs".chars() {
            let k = ev(0, c);
            let predicted = unsafe { vt_would_consume(e.0, id, &k) } != 0;
            assert_eq!(predicted, press(&e, id, 0, c).0, "{c}");
        }
    }

    #[test]
    fn secure_input_and_mode_switch_pass_keys_through() {
        let e = engine("");
        let secure = context(&e, "", 0, 1 << 4);
        assert!(!press(&e, secure, 0, 'a').0);

        let id = context(&e, "", 0, 1);
        let mut r = null_mut();
        assert_eq!(unsafe { vt_set_mode(e.0, id, 1, &mut r) }, VT_OK);
        let (_, a) = collect(r);
        assert_eq!(a, [(ACTION_MODE_CHANGED, String::new())]);
        assert_eq!(unsafe { vt_get_mode(e.0, id) }, 1);
        assert!(!press(&e, id, 0, 'a').0);
    }

    #[test]
    fn profile_selected_by_application_id() {
        let toml = r#"
[[profiles]]
id = "123e4567-e89b-12d3-a456-426614174000"
name = "vni app"
[profiles.match]
kind = "exe_name"
value = "code.exe"
[profiles.override]
scheme = "vni"
"#;
        let e = engine(toml);
        let vs = context(&e, "C:\\Program Files\\VS\\Code.exe", 1, 1);
        let other = context(&e, "notepad.exe", 1, 1);
        assert_eq!(type_str(&e, vs, "tie6ng ").last().unwrap().1, "tiêng");
        assert_eq!(type_str(&e, other, "tieeng ").last().unwrap().1, "tiêng");
    }

    #[test]
    fn error_codes() {
        unsafe {
            let mut e = null_mut();
            assert_eq!(vt_engine_new(null(), 0, null_mut()), VT_ERR_NULL);
            assert_eq!(vt_engine_new(null(), 5, &mut e), VT_ERR_NULL);
            assert_eq!(
                vt_engine_new([0xffu8].as_ptr(), 1, &mut e),
                VT_ERR_INVALID_UTF8
            );
            let bad = "[general]\nscheme = \"x\"";
            assert_eq!(
                vt_engine_new(bad.as_ptr(), bad.len(), &mut e),
                VT_ERR_CONFIG
            );
            assert!(e.is_null());
        }
        let e = engine("");
        let id = context(&e, "", 0, 1);
        let mut r = null_mut();
        unsafe {
            assert_eq!(vt_process_key(e.0, id, null(), &mut r), VT_ERR_NULL);
            assert_eq!(
                vt_process_key(e.0, 999, &ev(0, 'a'), &mut r),
                VT_ERR_UNKNOWN_CONTEXT
            );
            let mut bad = ev(0, 'a');
            bad.struct_size = 4;
            assert_eq!(vt_process_key(e.0, id, &bad, &mut r), VT_ERR_ABI_MISMATCH);
            assert_eq!(
                vt_process_key(e.0, id, &ev(99, 'a'), &mut r),
                VT_ERR_INVALID_ARG
            );
            let mut surrogate = ev(0, 'a');
            surrogate.ch = 0xD800;
            assert_eq!(
                vt_process_key(e.0, id, &surrogate, &mut r),
                VT_ERR_INVALID_ARG
            );
            assert_eq!(vt_notify(e.0, id, 77, &mut r), VT_ERR_INVALID_ARG);
            assert_eq!(vt_set_mode(e.0, id, 5, &mut r), VT_ERR_INVALID_ARG);
            assert_eq!(
                vt_process_key(null_mut(), id, &ev(0, 'a'), &mut r),
                VT_ERR_NULL
            );
            assert_eq!(
                vt_process_key(e.0, id, &ev(0, 'a'), null_mut()),
                VT_ERR_NULL
            );
            assert_eq!(vt_would_consume(null(), id, &ev(0, 'a')), 0);
            assert_eq!(vt_get_mode(null(), id), -1);
            assert_eq!(vt_get_mode(e.0, 12345), -1);
            // Result accessors tolerate NULL and out-of-range indexes.
            assert_eq!(vt_result_len(null()), 0);
            assert_eq!(vt_result_consumed(null()), 0);
            let mut a = std::mem::zeroed::<VtActionView>();
            assert_eq!(vt_result_get(null(), 0, &mut a), VT_ERR_NULL);
            vt_result_free(null_mut());
            vt_engine_free(null_mut());
            vt_context_destroy(null_mut(), 1);
        }
    }

    #[test]
    fn invalid_utf8_app_id_is_rejected() {
        let e = engine("");
        let info = VtContextInfo {
            struct_size: std::mem::size_of::<VtContextInfo>() as u32,
            app_id_utf8: [0xc3u8, 0x28].as_ptr(),
            app_id_len: 2,
            app_id_kind: 1,
            capabilities: 0,
        };
        let mut id = 0;
        assert_eq!(
            unsafe { vt_context_create(e.0, &info, &mut id) },
            VT_ERR_INVALID_UTF8
        );
    }

    #[test]
    fn destroyed_context_is_unknown() {
        let e = engine("");
        let id = context(&e, "", 0, 1);
        unsafe { vt_context_destroy(e.0, id) };
        let mut r = null_mut();
        assert_eq!(
            unsafe { vt_process_key(e.0, id, &ev(0, 'a'), &mut r) },
            VT_ERR_UNKNOWN_CONTEXT
        );
    }

    #[test]
    fn reload_config_changes_behaviour() {
        let e = engine("");
        let id = context(&e, "", 0, 1);
        let vni = "[general]\nscheme = \"vni\"\n";
        assert_eq!(
            unsafe { vt_engine_reload_config(e.0, vni.as_ptr(), vni.len()) },
            VT_OK
        );
        assert_eq!(type_str(&e, id, "tie6ng ").last().unwrap().1, "tiêng");
        let bad = "[general\n";
        assert_eq!(
            unsafe { vt_engine_reload_config(e.0, bad.as_ptr(), bad.len()) },
            VT_ERR_CONFIG
        );
        // The previous good settings stay.
        assert_eq!(type_str(&e, id, "tie6ng ").last().unwrap().1, "tiêng");
    }

    #[test]
    fn focus_update_returns_pending_commit() {
        let e = engine("");
        let id = context(&e, "", 0, 1);
        type_str(&e, id, "xin");
        let info = VtContextInfo {
            struct_size: std::mem::size_of::<VtContextInfo>() as u32,
            app_id_utf8: null(),
            app_id_len: 0,
            app_id_kind: 0,
            capabilities: 1,
        };
        let mut r = null_mut();
        assert_eq!(unsafe { vt_context_update(e.0, id, &info, &mut r) }, VT_OK);
        assert_eq!(collect(r).1, [(ACTION_COMMIT, "xin".to_string())]);
    }

    #[test]
    fn poisoned_engine_fails_open() {
        let e = engine("");
        let id = context(&e, "", 0, 1);
        unsafe { (*e.0).poisoned.store(true, Ordering::Relaxed) };
        let (consumed, actions) = press(&e, id, 0, 'a');
        assert!(!consumed && actions.is_empty());
        assert_eq!(unsafe { vt_would_consume(e.0, id, &ev(0, 'a')) }, 0);
    }

    #[test]
    fn panics_are_caught_and_poison_the_engine() {
        let e = engine("");
        let eng = unsafe { &*e.0 };
        let s = guarded(Some(&eng.poisoned), || panic!("boom"));
        assert_eq!(s, VT_ERR_PANIC);
        assert!(eng.poisoned.load(Ordering::Relaxed));
    }
}
