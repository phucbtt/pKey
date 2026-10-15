//! Edit sessions: TSF only lets a text service touch a document from inside one.

use std::cell::RefCell;

use windows::core::{implement, Result, HRESULT};
use windows::Win32::UI::TextServices::{
    ITfContext, ITfEditSession, ITfEditSession_Impl, TF_ES_ASYNCDONTCARE, TF_ES_READ,
    TF_ES_READWRITE, TF_ES_SYNC,
};

type Job = Box<dyn FnOnce(u32) -> Result<()>>;

#[implement(ITfEditSession)]
struct EditSession {
    job: RefCell<Option<Job>>,
}

impl ITfEditSession_Impl for EditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        let Some(job) = self.job.borrow_mut().take() else {
            return Ok(());
        };
        // A panic must not unwind through TSF.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(ec)))
            .unwrap_or_else(|_| super::fail())
    }
}

/// Run `job` synchronously inside a read/write edit session on `ctx`.
///
/// Returns the error TSF reports if the session could not run (the document is locked, the
/// context is gone, ...); the caller then tells the engine with `Notify::HostCommitFailed`.
pub fn run_sync(
    ctx: &ITfContext,
    client_id: u32,
    job: impl FnOnce(u32) -> Result<()> + 'static,
) -> Result<()> {
    let session: ITfEditSession = EditSession {
        job: RefCell::new(Some(Box::new(job))),
    }
    .into();
    // SAFETY: `session` and `ctx` are live COM objects; the call is synchronous.
    let hr: HRESULT =
        unsafe { ctx.RequestEditSession(client_id, &session, TF_ES_SYNC | TF_ES_READWRITE)? };
    hr.ok()
}

/// Run `job` in a read/write edit session as soon as the document allows it: at once when it
/// can, or after the lock currently held (for example inside `OnEndEdit`) is released.
pub fn run_when_possible(
    ctx: &ITfContext,
    client_id: u32,
    job: impl FnOnce(u32) -> Result<()> + 'static,
) -> Result<()> {
    let session: ITfEditSession = EditSession {
        job: RefCell::new(Some(Box::new(job))),
    }
    .into();
    // SAFETY: `session` and `ctx` are live COM objects.
    let hr: HRESULT = unsafe {
        ctx.RequestEditSession(client_id, &session, TF_ES_ASYNCDONTCARE | TF_ES_READWRITE)?
    };
    hr.ok()
}

/// Run `job` synchronously in a read-only edit session (enough to read properties).
pub fn read_sync(
    ctx: &ITfContext,
    client_id: u32,
    job: impl FnOnce(u32) -> Result<()> + 'static,
) -> Result<()> {
    let session: ITfEditSession = EditSession {
        job: RefCell::new(Some(Box::new(job))),
    }
    .into();
    // SAFETY: `session` and `ctx` are live COM objects; the call is synchronous.
    let hr: HRESULT =
        unsafe { ctx.RequestEditSession(client_id, &session, TF_ES_SYNC | TF_ES_READ)? };
    hr.ok()
}
