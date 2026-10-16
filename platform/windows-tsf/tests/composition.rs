//! Drives the real text service through real TSF against an in-memory text store.
//!
//! TSF itself (ITfThreadMgr, ITfContext, composition, ranges, properties) is the genuine
//! article; only the application's document is faked (`FakeStore`, an `ITextStoreACP`). The
//! test calls the text service's own key-sink methods, so nothing is registered with Windows and
//! no input method is installed.
#![cfg(windows)]

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

use windows::core::{implement, s, IUnknown, Interface, BOOL, GUID, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    E_NOTIMPL, E_UNEXPECTED, HWND, LPARAM, POINT, RECT, S_OK, WPARAM,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IClassFactory, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_VSC};
use windows::Win32::UI::TextServices::*;

const CLSID_PKEY: GUID = GUID::from_u128(0x434E620C_D5E7_4347_B6BC_BE6BE00C417D);

// ------------------------------------------------------------------ the fake document

#[derive(Default)]
struct Doc {
    text: RefCell<Vec<u16>>,
    sel: Cell<(i32, i32)>,
    sink: RefCell<Option<ITextStoreACPSink>>,
    /// The field is a password box: its input scope says so.
    secure: Cell<bool>,
    /// What the next `RetrieveRequestedAttrs` answers: 1 = which attributes exist, 2 = values.
    attr_phase: Cell<u8>,
    /// Every edit made through the store, for assertions about what TSF actually did.
    log: RefCell<Vec<String>>,
}

#[implement(ITextStoreACP)]
struct FakeStore {
    doc: Rc<Doc>,
}

fn change(start: i32, old_end: i32, new_end: i32) -> TS_TEXTCHANGE {
    TS_TEXTCHANGE {
        acpStart: start,
        acpOldEnd: old_end,
        acpNewEnd: new_end,
    }
}

impl FakeStore {
    fn replace(&self, start: i32, end: i32, new: &[u16]) -> TS_TEXTCHANGE {
        let mut t = self.doc.text.borrow_mut();
        let end = if end < 0 { t.len() as i32 } else { end };
        let (s, e) = (start as usize, end as usize);
        t.splice(s..e, new.iter().copied());
        let new_end = start + new.len() as i32;
        // Keep the caret where an editor would: collapsed after inserted text.
        let (ss, se) = self.doc.sel.get();
        let shift = |p: i32| {
            if p >= end {
                p + new.len() as i32 - (end - start)
            } else {
                p.min(start)
            }
        };
        let _ = (ss, se);
        self.doc.sel.set((shift(ss), shift(se)));
        self.doc.log.borrow_mut().push(format!(
            "replace {start}..{end} with {:?}",
            String::from_utf16_lossy(new)
        ));
        change(start, end, new_end)
    }
}

#[allow(non_snake_case)]
impl ITextStoreACP_Impl for FakeStore_Impl {
    fn AdviseSink(
        &self,
        riid: *const GUID,
        punk: windows::core::Ref<'_, IUnknown>,
        _mask: u32,
    ) -> windows::core::Result<()> {
        // SAFETY: TSF passes a valid iid pointer.
        assert_eq!(unsafe { *riid }, ITextStoreACPSink::IID);
        *self.doc.sink.borrow_mut() = Some(punk.ok()?.cast()?);
        Ok(())
    }

    fn UnadviseSink(&self, _punk: windows::core::Ref<'_, IUnknown>) -> windows::core::Result<()> {
        *self.doc.sink.borrow_mut() = None;
        Ok(())
    }

    fn RequestLock(&self, flags: u32) -> windows::core::Result<HRESULT> {
        let sink = self.doc.sink.borrow().clone().ok_or(E_UNEXPECTED)?;
        // Grant the lock synchronously; TSF runs the edit session inside OnLockGranted.
        let r = unsafe { sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(flags)) };
        Ok(r.map_or_else(|e| e.code(), |_| S_OK))
    }

    fn GetStatus(&self) -> windows::core::Result<TS_STATUS> {
        Ok(TS_STATUS::default())
    }

    fn QueryInsert(
        &self,
        start: i32,
        end: i32,
        _cch: u32,
        pstart: *mut i32,
        pend: *mut i32,
    ) -> windows::core::Result<()> {
        unsafe {
            *pstart = start;
            *pend = end;
        }
        Ok(())
    }

    fn GetSelection(
        &self,
        _index: u32,
        _count: u32,
        sel: *mut TS_SELECTION_ACP,
        fetched: *mut u32,
    ) -> windows::core::Result<()> {
        let (s, e) = self.doc.sel.get();
        unsafe {
            *sel = TS_SELECTION_ACP {
                acpStart: s,
                acpEnd: e,
                style: TS_SELECTIONSTYLE {
                    ase: TS_AE_END,
                    fInterimChar: false.into(),
                },
            };
            *fetched = 1;
        }
        Ok(())
    }

    fn SetSelection(&self, count: u32, sel: *const TS_SELECTION_ACP) -> windows::core::Result<()> {
        assert_eq!(count, 1);
        let s = unsafe { *sel };
        self.doc.sel.set((s.acpStart, s.acpEnd));
        self.doc
            .log
            .borrow_mut()
            .push(format!("select {}..{}", s.acpStart, s.acpEnd));
        Ok(())
    }

    fn GetText(
        &self,
        start: i32,
        end: i32,
        plain: PWSTR,
        plain_req: u32,
        plain_ret: *mut u32,
        runs: *mut TS_RUNINFO,
        runs_req: u32,
        runs_ret: *mut u32,
        next: *mut i32,
    ) -> windows::core::Result<()> {
        let t = self.doc.text.borrow();
        let end = if end < 0 { t.len() as i32 } else { end };
        let n = ((end - start).max(0) as usize).min(plain_req as usize);
        unsafe {
            for i in 0..n {
                *plain.0.add(i) = t[start as usize + i];
            }
            *plain_ret = n as u32;
            *runs_ret = 0;
            if runs_req > 0 && n > 0 {
                *runs = TS_RUNINFO {
                    uCount: n as u32,
                    r#type: TS_RT_PLAIN,
                };
                *runs_ret = 1;
            }
            *next = start + n as i32;
        }
        Ok(())
    }

    fn SetText(
        &self,
        _flags: u32,
        start: i32,
        end: i32,
        text: &windows::core::PCWSTR,
        cch: u32,
    ) -> windows::core::Result<TS_TEXTCHANGE> {
        let new = if cch == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(text.0, cch as usize) }
        };
        Ok(self.replace(start, end, new))
    }

    fn GetFormattedText(
        &self,
        _s: i32,
        _e: i32,
    ) -> windows::core::Result<windows::Win32::System::Com::IDataObject> {
        Err(E_NOTIMPL.into())
    }

    fn GetEmbedded(
        &self,
        _p: i32,
        _g: *const GUID,
        _r: *const GUID,
    ) -> windows::core::Result<IUnknown> {
        Err(E_NOTIMPL.into())
    }

    fn QueryInsertEmbedded(
        &self,
        _g: *const GUID,
        _f: *const windows::Win32::System::Com::FORMATETC,
    ) -> windows::core::Result<BOOL> {
        Ok(false.into())
    }

    fn InsertEmbedded(
        &self,
        _f: u32,
        _s: i32,
        _e: i32,
        _o: windows::core::Ref<'_, windows::Win32::System::Com::IDataObject>,
    ) -> windows::core::Result<TS_TEXTCHANGE> {
        Err(E_NOTIMPL.into())
    }

    fn InsertTextAtSelection(
        &self,
        flags: u32,
        text: &windows::core::PCWSTR,
        cch: u32,
        pstart: *mut i32,
        pend: *mut i32,
        pchange: *mut TS_TEXTCHANGE,
    ) -> windows::core::Result<()> {
        let (s, e) = self.doc.sel.get();
        if flags & TS_IAS_QUERYONLY != 0 {
            unsafe {
                *pstart = s;
                *pend = e;
            }
            return Ok(());
        }
        let new = if cch == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(text.0, cch as usize) }
        };
        let c = self.replace(s, e, new);
        unsafe {
            if !pstart.is_null() {
                *pstart = c.acpStart;
            }
            if !pend.is_null() {
                *pend = c.acpNewEnd;
            }
            if !pchange.is_null() {
                *pchange = c;
            }
        }
        Ok(())
    }

    fn InsertEmbeddedAtSelection(
        &self,
        _f: u32,
        _o: windows::core::Ref<'_, windows::Win32::System::Com::IDataObject>,
        _s: *mut i32,
        _e: *mut i32,
        _c: *mut TS_TEXTCHANGE,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn RequestSupportedAttrs(
        &self,
        _f: u32,
        _c: u32,
        _a: *const GUID,
    ) -> windows::core::Result<()> {
        self.doc.attr_phase.set(1);
        Ok(())
    }

    fn RequestAttrsAtPosition(
        &self,
        _p: i32,
        _c: u32,
        _a: *const GUID,
        _f: u32,
    ) -> windows::core::Result<()> {
        self.doc.attr_phase.set(2);
        Ok(())
    }

    fn RequestAttrsTransitioningAtPosition(
        &self,
        _p: i32,
        _c: u32,
        _a: *const GUID,
        _f: u32,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn FindNextAttrTransition(
        &self,
        _s: i32,
        _h: i32,
        _c: u32,
        _a: *const GUID,
        _f: u32,
        pnext: *mut i32,
        pfound: *mut BOOL,
        poffset: *mut i32,
    ) -> windows::core::Result<()> {
        unsafe {
            *pnext = 0;
            *pfound = false.into();
            *poffset = 0;
        }
        Ok(())
    }

    fn RetrieveRequestedAttrs(
        &self,
        count: u32,
        vals: *mut TS_ATTRVAL,
        fetched: *mut u32,
    ) -> windows::core::Result<()> {
        unsafe {
            *fetched = 0;
            if count == 0 {
                return Ok(());
            }
            let value = if self.doc.attr_phase.get() == 2 {
                // The value: an input scope that says password, or an ordinary one.
                let scope: ITfInputScope = FakeScope {
                    scope: if self.doc.secure.get() {
                        IS_PASSWORD
                    } else {
                        IS_DEFAULT
                    },
                }
                .into();
                let unknown: IUnknown = scope.cast()?;
                windows::Win32::System::Variant::VARIANT::from(unknown)
            } else {
                // "This attribute is supported", no value yet.
                windows::Win32::System::Variant::VARIANT::default()
            };
            vals.write(TS_ATTRVAL {
                idAttr: GUID_PROP_INPUTSCOPE,
                dwOverlapId: 0,
                varValue: value,
            });
            *fetched = 1;
        }
        Ok(())
    }

    fn GetEndACP(&self) -> windows::core::Result<i32> {
        Ok(self.doc.text.borrow().len() as i32)
    }

    fn GetActiveView(&self) -> windows::core::Result<u32> {
        Ok(0)
    }

    fn GetACPFromPoint(&self, _v: u32, _p: *const POINT, _f: u32) -> windows::core::Result<i32> {
        Err(E_NOTIMPL.into())
    }

    fn GetTextExt(
        &self,
        _v: u32,
        _s: i32,
        _e: i32,
        _r: *mut RECT,
        _c: *mut BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn GetScreenExt(&self, _v: u32) -> windows::core::Result<RECT> {
        Err(E_NOTIMPL.into())
    }

    fn GetWnd(&self, _v: u32) -> windows::core::Result<HWND> {
        Ok(HWND::default())
    }
}

#[implement(ITfInputScope)]
struct FakeScope {
    scope: InputScope,
}

#[allow(non_snake_case)]
impl ITfInputScope_Impl for FakeScope_Impl {
    fn GetInputScopes(
        &self,
        scopes: *mut *mut InputScope,
        count: *mut u32,
    ) -> windows::core::Result<()> {
        unsafe {
            let p = windows::Win32::System::Com::CoTaskMemAlloc(std::mem::size_of::<InputScope>())
                as *mut InputScope;
            p.write(self.scope);
            *scopes = p;
            *count = 1;
        }
        Ok(())
    }

    fn GetPhrase(
        &self,
        _out: *mut *mut windows::core::BSTR,
        _c: *mut u32,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn GetRegularExpression(&self) -> windows::core::Result<windows::core::BSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetSRGS(&self) -> windows::core::Result<windows::core::BSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetXML(&self) -> windows::core::Result<windows::core::BSTR> {
        Err(E_NOTIMPL.into())
    }
}

// ------------------------------------------------------------------ test harness

fn build_dll() -> PathBuf {
    // `target/<profile>` normally, `target/<triple>/<profile>` when testing with `--target`.
    let exe = std::env::current_exe().unwrap();
    let profile_dir = exe.parent().unwrap().parent().unwrap().to_path_buf();
    let outer = profile_dir.parent().unwrap();
    let triple = outer
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| n.ends_with("-pc-windows-msvc"));
    let target_dir = if triple.is_some() {
        outer.parent().unwrap()
    } else {
        outer
    };
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.args([
        "build",
        "-p",
        "pkey-tsf",
        "--lib",
        "--features",
        "test-hooks",
    ])
    .env("CARGO_TARGET_DIR", target_dir);
    if let Some(t) = triple {
        cmd.args(["--target", t]);
    }
    if profile_dir.file_name().is_some_and(|n| n == "release") {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("cargo build");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    profile_dir.join("pkey_tsf.dll")
}

type GetClassObject =
    unsafe extern "system" fn(*const GUID, *const GUID, *mut *mut c_void) -> HRESULT;

/// The text service keeps one runtime per process, so scenarios must not overlap.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Document {
    dm: ITfDocumentMgr,
    ctx: ITfContext,
    doc: Rc<Doc>,
}

struct Fixture {
    _serial: std::sync::MutexGuard<'static, ()>,
    tip: ITfTextInputProcessorEx,
    key_sink: ITfKeyEventSink,
    thread_mgr: ITfThreadMgr,
    client_id: u32,
    documents: RefCell<Vec<Document>>,
    active: Cell<usize>,
}

fn note<T, E: std::fmt::Display>(r: Result<T, E>, what: &str) -> Option<T> {
    r.map_err(|e| eprintln!("setup step `{what}` failed: {e}"))
        .ok()
}

/// A fresh document with its own fake text store, registered with TSF.
fn new_document(thread_mgr: &ITfThreadMgr, client_id: u32) -> Option<Document> {
    unsafe {
        let dm = note(thread_mgr.CreateDocumentMgr(), "CreateDocumentMgr")?;
        let doc = Rc::new(Doc::default());
        let store: ITextStoreACP = FakeStore {
            doc: Rc::clone(&doc),
        }
        .into();
        let store_unknown: IUnknown = note(store.cast(), "store_unknown")?;
        let mut ctx = None;
        let mut cookie = 0;
        note(
            dm.CreateContext(client_id, 0, &store_unknown, &mut ctx, &mut cookie),
            "CreateContext",
        )?;
        let ctx = ctx?;
        note(dm.Push(&ctx), "Push")?;
        Some(Document { dm, ctx, doc })
    }
}

impl Fixture {
    /// `None` when this machine cannot host the experiment (reported, not failed).
    fn new() -> Option<Fixture> {
        let serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        // The text service reads its settings from here instead of the real user profile.
        // Nothing is ever written there (no settings change), so it is not even created.
        let dir = std::env::temp_dir().join(format!("pkey-tsf-test-{}", std::process::id()));
        std::env::set_var("PKEY_CONFIG_DIR", &dir);

        let path = build_dll();
        let wide: Vec<u16> = path.to_string_lossy().encode_utf16().chain([0]).collect();
        unsafe {
            note(
                CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok(),
                "CoInitializeEx(None, COINIT_APARTMENTTHR",
            )?;
            let module = note(LoadLibraryW(PCWSTR(wide.as_ptr())), "module")?;
            let get: GetClassObject =
                std::mem::transmute(GetProcAddress(module, s!("DllGetClassObject"))?);
            let mut raw = std::ptr::null_mut();
            assert_eq!(get(&CLSID_PKEY, &IClassFactory::IID, &mut raw), S_OK);
            let factory = IClassFactory::from_raw(raw);
            let tip: ITfTextInputProcessorEx = note(factory.CreateInstance(None), "tip")?;

            let thread_mgr: ITfThreadMgr = note(
                CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER),
                "thread_mgr",
            )?;
            // The id TSF would give this text service once it is activated for the thread.
            let tip_id = note(
                thread_mgr
                    .cast::<ITfClientId>()
                    .ok()?
                    .GetClientId(&CLSID_PKEY),
                "tip_id",
            )?;
            let client_id = note(thread_mgr.Activate(), "thread_mgr.Activate()")?;
            let first = new_document(&thread_mgr, client_id)?;
            note(thread_mgr.SetFocus(&first.dm), "SetFocus")?;

            if let Err(e) = tip.ActivateEx(&thread_mgr, tip_id, 0) {
                eprintln!("SKIPPED: ActivateEx failed: {e}");
                return None;
            }
            let key_sink = tip.cast::<ITfKeyEventSink>().ok()?;
            Some(Fixture {
                _serial: serial,
                tip,
                key_sink,
                thread_mgr,
                client_id,
                documents: RefCell::new(vec![first]),
                active: Cell::new(0),
            })
        }
    }

    fn ctx(&self) -> ITfContext {
        self.documents.borrow()[self.active.get()].ctx.clone()
    }

    fn doc(&self) -> Rc<Doc> {
        Rc::clone(&self.documents.borrow()[self.active.get()].doc)
    }

    /// Open another document (another text field) and return its index.
    fn add_document(&self) -> usize {
        let d = new_document(&self.thread_mgr, self.client_id).expect("second document");
        self.documents.borrow_mut().push(d);
        self.documents.borrow().len() - 1
    }

    /// Give another document the focus, as clicking into another window would.
    fn focus(&self, index: usize) {
        self.active.set(index);
        let dm = self.documents.borrow()[index].dm.clone();
        unsafe { self.thread_mgr.SetFocus(&dm).unwrap() };
    }

    fn text(&self) -> String {
        String::from_utf16_lossy(&self.doc().text.borrow())
    }

    /// Press and release one key through the text service the way TSF would: test, do, up.
    /// Returns whether the key was eaten.
    fn press(&self, vk: u32) -> bool {
        let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC) };
        let down = LPARAM((1 | (scan << 16)) as isize);
        let up = LPARAM((1 | (scan << 16) | (1 << 30) | (1 << 31)) as i32 as isize);
        let w = WPARAM(vk as usize);
        unsafe {
            let route = self
                .key_sink
                .OnTestKeyDown(&self.ctx(), w, down)
                .unwrap()
                .as_bool();
            let eaten = if route {
                self.key_sink
                    .OnKeyDown(&self.ctx(), w, down)
                    .unwrap()
                    .as_bool()
            } else {
                false
            };
            if self
                .key_sink
                .OnTestKeyUp(&self.ctx(), w, up)
                .unwrap()
                .as_bool()
            {
                let _ = self.key_sink.OnKeyUp(&self.ctx(), w, up);
            }
            eaten
        }
    }

    /// One physical key event: `down` or release. Returns whether the key was eaten.
    fn key_event(&self, vk: u32, down: bool) -> bool {
        let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC) };
        let mut lp = 1u32 | (scan << 16);
        if !down {
            lp |= (1 << 30) | (1 << 31);
        }
        let (w, l) = (WPARAM(vk as usize), LPARAM(lp as i32 as isize));
        unsafe {
            if down {
                let route = self
                    .key_sink
                    .OnTestKeyDown(&self.ctx(), w, l)
                    .unwrap()
                    .as_bool();
                route
                    && self
                        .key_sink
                        .OnKeyDown(&self.ctx(), w, l)
                        .unwrap()
                        .as_bool()
            } else {
                if self
                    .key_sink
                    .OnTestKeyUp(&self.ctx(), w, l)
                    .unwrap()
                    .as_bool()
                {
                    let _ = self.key_sink.OnKeyUp(&self.ctx(), w, l);
                }
                false
            }
        }
    }

    /// Type text the way a person would: letters by their key, space by the space bar. For a
    /// key that the application itself would handle (not eaten) the character is inserted into
    /// the document, as the application would.
    fn type_text(&self, s: &str) {
        for c in s.chars() {
            let vk = if c == ' ' {
                0x20
            } else {
                c.to_ascii_uppercase() as u32
            };
            let eaten = self.press(vk);
            if !eaten {
                self.app_inserts(c);
            }
        }
    }

    /// What an application does with a key we did not eat.
    fn app_inserts(&self, c: char) {
        let doc = self.doc();
        let (s, e) = doc.sel.get();
        let mut t = doc.text.borrow_mut();
        let (s, e) = (s as usize, e as usize);
        let mut buf = [0u16; 2];
        let units = c.encode_utf16(&mut buf);
        let n = units.len();
        t.splice(s..e, units.iter().copied());
        let p = (s + n) as i32;
        doc.sel.set((p, p));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.tip.Deactivate();
            let _ = self.thread_mgr.Deactivate();
            CoUninitialize();
        }
    }
}

/// The test machine's keyboard layout must turn letter keys into the same Latin letters.
fn layout_is_latin() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, ToUnicodeEx};
    let state = [0u8; 256];
    let mut buf = [0u16; 4];
    let n = unsafe {
        let scan = MapVirtualKeyW(0x41, MAPVK_VK_TO_VSC);
        ToUnicodeEx(
            0x41,
            scan,
            &state,
            &mut buf,
            0x4,
            Some(GetKeyboardLayout(0)),
        )
    };
    n == 1 && buf[0] == u16::from(b'a')
}

#[test]
fn typing_composes_underlines_and_commits_in_a_real_tsf_context() {
    let Some(f) = fixture_or_skip() else { return };

    // While typing, the word is composition text in the document.
    f.type_text("tieengs");
    assert_eq!(f.text(), "tiếng");
    // Space ends the word: committed, then the application inserts the space itself.
    f.type_text(" ");
    assert_eq!(f.text(), "tiếng ");

    f.type_text("vieejt ");
    assert_eq!(f.text(), "tiếng việt ");

    // Backspace inside a composition edits the composition, and an English word is left alone.
    f.type_text("tooi");
    assert_eq!(f.text(), "tiếng việt tôi");
    f.press(0x08); // Backspace is eaten while composing
    assert_eq!(f.text(), "tiếng việt tô");
    f.type_text(" ");
    assert_eq!(f.text(), "tiếng việt tô ");

    f.type_text("windows ");
    assert_eq!(f.text(), "tiếng việt tô windows ");

    eprintln!("final document: {:?}", f.text());
    let doc = f.doc();
    let log = doc.log.borrow();
    eprintln!(
        "{} store edits, first 12: {:#?}",
        log.len(),
        &log[..log.len().min(12)]
    );
    // Composition text went through TSF ranges (SetText), not through raw insertion.
    assert!(log.iter().any(|l| l.starts_with("replace")));
}

/// `PKEY_REQUIRE_TSF_TESTS=1` (CI) turns "cannot run here" into a failure.
fn skip(why: &str) {
    assert!(
        std::env::var_os("PKEY_REQUIRE_TSF_TESTS").is_none(),
        "TSF test required but cannot run: {why}"
    );
    eprintln!("SKIPPED: {why}");
}

fn fixture_or_skip() -> Option<Fixture> {
    if !layout_is_latin() {
        skip("the keyboard layout does not map the A key to 'a'");
        return None;
    }
    let f = Fixture::new();
    if f.is_none() {
        skip("could not set up a TSF context in this session");
    }
    f
}

/// Current COM reference count of the object behind `i` (AddRef returns the new count).
fn ref_count<T: Interface>(i: &T) -> u32 {
    // SAFETY: every COM interface pointer starts with a pointer to a table whose first three
    // entries are the `IUnknown` methods.
    unsafe {
        let raw = i.as_raw();
        let vtbl = *(raw as *const *const windows::core::IUnknown_Vtbl);
        let n = ((*vtbl).AddRef)(raw);
        ((*vtbl).Release)(raw);
        n - 1
    }
}

/// `Deactivate` must drop everything the service holds on itself (the sinks it handed to TSF and
/// keeps for later use). Before the fix those two strong references formed a cycle with the
/// service, so it was never released and the DLL could never unload from the host.
#[test]
fn deactivating_releases_every_reference_the_service_took() {
    let Some(f) = fixture_or_skip() else { return };
    // Use the service enough to create a context (which advises the edit sink) and a composition.
    f.type_text("tiee");
    f.type_text(" ");
    let active = ref_count(&f.tip);
    unsafe { f.tip.Deactivate().unwrap() };
    let after = ref_count(&f.tip);
    eprintln!("references while active: {active}, after Deactivate: {after}");
    // The test itself keeps two (`tip` and `key_sink`); nothing else may be left.
    assert_eq!(
        after,
        2,
        "the service still holds {} reference(s) to itself after Deactivate",
        after.saturating_sub(2)
    );
}

#[test]
fn a_composition_ended_by_the_host_is_forgotten() {
    let Some(f) = fixture_or_skip() else { return };
    f.type_text("tie");
    assert_eq!(f.text(), "tie");
    // The application (or TSF) ends our composition behind our back.
    let owner: ITfContextOwnerCompositionServices = f.ctx().cast().unwrap();
    unsafe { owner.TerminateComposition(None).unwrap() };
    // If the engine still believed it was composing "tie", this x would turn it into "tiẽ".
    f.type_text("x");
    assert_eq!(f.text(), "tiex");
}

#[test]
fn ctrl_shift_toggles_between_vietnamese_and_english() {
    let Some(f) = fixture_or_skip() else { return };
    const CTRL: u32 = 0x11;
    const SHIFT: u32 = 0x10;
    let toggle = |f: &Fixture| {
        f.key_event(CTRL, true);
        f.key_event(SHIFT, true);
        f.key_event(SHIFT, false);
        f.key_event(CTRL, false);
    };
    f.type_text("tieengs ");
    assert_eq!(f.text(), "tiếng ");
    toggle(&f);
    f.type_text("tieengs ");
    assert_eq!(f.text(), "tiếng tieengs ", "English mode leaves keys alone");
    toggle(&f);
    f.type_text("tieengs ");
    assert_eq!(f.text(), "tiếng tieengs tiếng ");
}

#[test]
fn toggling_commits_the_word_in_progress() {
    let Some(f) = fixture_or_skip() else { return };
    f.type_text("vieejt");
    assert_eq!(f.text(), "việt");
    for (vk, down) in [(0x11, true), (0x10, true), (0x10, false), (0x11, false)] {
        f.key_event(vk, down);
    }
    // The word stayed (committed), and typing continues in English.
    assert_eq!(f.text(), "việt");
    f.type_text("x");
    assert_eq!(f.text(), "việtx");
    for (vk, down) in [(0x11, true), (0x10, true), (0x10, false), (0x11, false)] {
        f.key_event(vk, down);
    }
}

#[test]
fn ctrl_shift_with_another_key_in_between_does_not_toggle() {
    let Some(f) = fixture_or_skip() else { return };
    // Ctrl+Shift+T (for example "reopen closed tab") must not switch the mode.
    f.key_event(0x11, true);
    f.key_event(0x10, true);
    f.key_event(0x54, true);
    f.key_event(0x54, false);
    f.key_event(0x10, false);
    f.key_event(0x11, false);
    // Still Vietnamese: a fresh word composes. (The T above reached the engine as a letter
    // because the test cannot really hold Ctrl; close that word first.)
    f.type_text(" ");
    let before = f.text();
    f.type_text("vieejt ");
    assert_eq!(f.text(), format!("{before}việt "));
}

#[test]
fn moving_the_caret_away_ends_the_composition() {
    let Some(f) = fixture_or_skip() else { return };
    f.type_text("tie");
    assert_eq!(f.text(), "tie");
    // The user clicks at the start of the document: the application moves the selection and
    // tells TSF. The word being composed is finished where it stands.
    f.doc().sel.set((0, 0));
    let sink = f.doc().sink.borrow().clone().unwrap();
    unsafe { sink.OnSelectionChange().unwrap() };
    // A new word at the new caret position. If the engine still composed "tie", the x would
    // have rewritten it to "tiẽ" instead.
    f.type_text("x");
    assert_eq!(f.text(), "xtie");
}

#[test]
fn the_mode_is_shared_between_documents() {
    let Some(f) = fixture_or_skip() else { return };
    let second = f.add_document();
    f.type_text("vieejt ");
    assert_eq!(f.text(), "việt ");

    // Switch to English in the first document ...
    for (vk, down) in [(0x11, true), (0x10, true), (0x10, false), (0x11, false)] {
        f.key_event(vk, down);
    }
    // ... and the other document follows when it gets the focus.
    f.focus(second);
    f.type_text("vieejt ");
    assert_eq!(f.text(), "vieejt ", "English is shared");

    // Toggling back there carries over as well.
    for (vk, down) in [(0x11, true), (0x10, true), (0x10, false), (0x11, false)] {
        f.key_event(vk, down);
    }
    f.focus(0);
    f.type_text("tieengs ");
    assert_eq!(f.text(), "việt tiếng ");
}

#[test]
fn password_fields_are_left_alone() {
    let Some(f) = fixture_or_skip() else { return };
    // An ordinary field first, to show the engine is active.
    f.type_text("tieengs ");
    assert_eq!(f.text(), "tiếng ");
    // The user tabs into a password box (same TSF context, now marked as a password).
    f.doc().secure.set(true);
    f.type_text("tieengs");
    assert_eq!(
        f.text(),
        "tiếng tieengs",
        "no Vietnamese composition in a password field"
    );
    // Back to a normal field.
    f.doc().secure.set(false);
    f.type_text(" tieengs ");
    assert_eq!(f.text(), "tiếng tieengs tiếng ");
}
