//! COM side of the text service. Everything runs in a single-threaded apartment (STA), one
//! `TextService` per UI thread, all sharing one process-wide [`ime_runtime::Runtime`].

/// Diagnostics for the integration tests; compiled out of production builds.
macro_rules! trace {
    ($($arg:tt)*) => {
        #[cfg(feature = "test-hooks")]
        eprintln!("[tsf] {}", format!($($arg)*));
    };
}

mod display;
mod factory;
mod guids;
mod registry;
mod service;
mod session;

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use windows::core::{Interface, GUID, HRESULT};
use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, E_FAIL, E_INVALIDARG, E_POINTER, HINSTANCE, HMODULE, S_FALSE, S_OK,
};
use windows::Win32::System::Com::IClassFactory;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

/// Handle of this DLL, set by `DllMain`.
static MODULE: AtomicIsize = AtomicIsize::new(0);
/// Live COM objects plus `LockServer` locks; `DllCanUnloadNow` reports `S_OK` at zero.
pub(crate) static LIVE_OBJECTS: AtomicU32 = AtomicU32::new(0);

fn module() -> HMODULE {
    HMODULE(MODULE.load(Ordering::Relaxed) as *mut c_void)
}

/// Full path of this DLL.
pub(crate) fn dll_path() -> Option<String> {
    let mut buf = [0u16; 1024];
    // SAFETY: `buf` is a valid writable buffer for the length passed.
    let n = unsafe { GetModuleFileNameW(Some(module()), &mut buf) } as usize;
    (n > 0 && n < buf.len()).then(|| String::from_utf16_lossy(&buf[..n]))
}

/// Run `f`, turning a panic into `E_FAIL`: a panic must never unwind into the host process.
pub(crate) fn guard<T>(
    default: windows::core::Result<T>,
    f: impl FnOnce() -> windows::core::Result<T>,
) -> windows::core::Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(default)
}

pub(crate) fn fail<T>() -> windows::core::Result<T> {
    Err(E_FAIL.into())
}

#[no_mangle]
pub extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _reserved: *mut c_void) -> i32 {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        MODULE.store(hinst.0 as isize, Ordering::Relaxed);
    }
    1
}

/// # Safety
/// Called by COM with valid pointers (or NULL, which is rejected).
#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return E_POINTER;
    }
    *ppv = std::ptr::null_mut();
    let result = std::panic::catch_unwind(|| {
        if *rclsid != guids::CLSID_PKEY {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = factory::ClassFactory::new().into();
        factory.query(riid, ppv)
    });
    result.unwrap_or(E_FAIL)
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if LIVE_OBJECTS.load(Ordering::Relaxed) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

fn register(scope: registry::Scope) -> HRESULT {
    let Some(path) = dll_path() else {
        return E_FAIL;
    };
    let r = std::panic::catch_unwind(|| {
        registry::register_server(scope, &path)?;
        registry::register_profile(&path)
    });
    match r {
        Ok(Ok(())) => S_OK,
        Ok(Err(e)) => e.code(),
        Err(_) => E_FAIL,
    }
}

fn unregister(scope: registry::Scope) -> HRESULT {
    let r = std::panic::catch_unwind(|| {
        // Remove the profile first (best effort), then the COM registration.
        let _ = registry::unregister_profile();
        registry::unregister_server(scope)
    });
    match r {
        Ok(Ok(())) => S_OK,
        Ok(Err(e)) => e.code(),
        Err(_) => E_FAIL,
    }
}

/// `regsvr32 pkey_tsf.dll`: register for all users (needs administrator rights).
#[no_mangle]
pub extern "system" fn DllRegisterServer() -> HRESULT {
    register(registry::Scope::Machine)
}

#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> HRESULT {
    unregister(registry::Scope::Machine)
}

/// `regsvr32 /n /i:user pkey_tsf.dll` registers for the current user only (developer use,
/// no administrator rights); `/n /i:user /u` removes it. `/i:machine` is the same as
/// `DllRegisterServer`.
///
/// # Safety
/// `cmd_line` is NULL or a NUL-terminated UTF-16 string, as passed by `regsvr32`.
#[no_mangle]
pub unsafe extern "system" fn DllInstall(install: i32, cmd_line: *const u16) -> HRESULT {
    let arg = if cmd_line.is_null() {
        String::new()
    } else {
        let mut len = 0;
        while *cmd_line.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(cmd_line, len))
    };
    let scope = match arg.trim().to_ascii_lowercase().as_str() {
        "user" => registry::Scope::User,
        "" | "machine" => registry::Scope::Machine,
        _ => return E_INVALIDARG,
    };
    if install != 0 {
        register(scope)
    } else {
        unregister(scope)
    }
}

#[allow(dead_code)]
fn _assert_interface<T: Interface>() {}
