use std::ffi::c_void;
use std::sync::atomic::Ordering;

use windows::core::{implement, Interface, Ref, Result, BOOL, GUID};
use windows::Win32::Foundation::{CLASS_E_NOAGGREGATION, E_POINTER};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};

use super::{service::TextService, LIVE_OBJECTS};

#[implement(IClassFactory)]
pub struct ClassFactory;

impl ClassFactory {
    pub fn new() -> ClassFactory {
        LIVE_OBJECTS.fetch_add(1, Ordering::Relaxed);
        ClassFactory
    }
}

impl Drop for ClassFactory {
    fn drop(&mut self) {
        LIVE_OBJECTS.fetch_sub(1, Ordering::Relaxed);
    }
}

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, windows::core::IUnknown>,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> Result<()> {
        if ppv.is_null() || riid.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: `ppv` was checked for NULL; COM requires it to be writable.
        unsafe { *ppv = std::ptr::null_mut() };
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        super::guard(super::fail(), || {
            let obj: windows::Win32::UI::TextServices::ITfTextInputProcessorEx =
                TextService::new().into();
            // SAFETY: `riid`/`ppv` are valid per the checks above.
            unsafe { obj.query(riid, ppv).ok() }
        })
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            LIVE_OBJECTS.fetch_add(1, Ordering::Relaxed);
        } else {
            LIVE_OBJECTS.fetch_sub(1, Ordering::Relaxed);
        }
        Ok(())
    }
}
