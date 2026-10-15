//! COM and TSF registration (spec §14.1). The profile is registered but not forced on: the user
//! adds the keyboard in Settings › Time & language.

use windows::core::{Error, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, E_FAIL, WIN32_ERROR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
use windows::Win32::UI::TextServices::{
    CLSID_TF_CategoryMgr, CLSID_TF_InputProcessorProfiles, ITfCategoryMgr,
    ITfInputProcessorProfileMgr, GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
    GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT, GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
    GUID_TFCAT_TIPCAP_UIELEMENTENABLED, GUID_TFCAT_TIP_KEYBOARD,
};

use super::guids::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// `HKLM\Software\Classes`: all users, needs administrator rights (installer).
    Machine,
    /// `HKCU\Software\Classes`: the current user only (development).
    User,
}

fn root(scope: Scope) -> HKEY {
    match scope {
        Scope::Machine => HKEY_LOCAL_MACHINE,
        Scope::User => HKEY_CURRENT_USER,
    }
}

fn check(e: WIN32_ERROR) -> Result<()> {
    if e == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(Error::from(e.to_hresult()))
    }
}

/// Create (or open) `subkey` under `root` and set its default value, or `name` if given.
fn set_string(root: HKEY, subkey: &str, name: Option<&str>, value: &str) -> Result<()> {
    let mut key = HKEY::default();
    // SAFETY: all pointers refer to live local data for the duration of the calls.
    unsafe {
        check(RegCreateKeyExW(
            root,
            &HSTRING::from(subkey),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        ))?;
        let data: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2);
        let name = name.map(HSTRING::from);
        let r = RegSetValueExW(
            key,
            name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr())),
            None,
            REG_SZ,
            Some(bytes),
        );
        let _ = RegCloseKey(key);
        check(r)
    }
}

fn clsid_key() -> String {
    format!("Software\\Classes\\CLSID\\{CLSID_STRING}")
}

/// Write the `CLSID\{...}\InprocServer32` registration.
pub fn register_server(scope: Scope, dll_path: &str) -> Result<()> {
    let r = root(scope);
    set_string(r, &clsid_key(), None, PROFILE_DESCRIPTION)?;
    let inproc = format!("{}\\InprocServer32", clsid_key());
    set_string(r, &inproc, None, dll_path)?;
    set_string(r, &inproc, Some("ThreadingModel"), "Apartment")
}

pub fn unregister_server(scope: Scope) -> Result<()> {
    // SAFETY: plain registry call with an owned string.
    let e = unsafe { RegDeleteTreeW(root(scope), &HSTRING::from(clsid_key())) };
    // A missing key is fine: unregistering twice must succeed.
    if e == ERROR_SUCCESS || e.0 == 2 {
        Ok(())
    } else {
        Err(Error::from(e.to_hresult()))
    }
}

/// Initialise COM for the registration calls, balanced on drop.
struct ComScope(bool);

impl ComScope {
    fn new() -> ComScope {
        // SAFETY: balanced by `CoUninitialize` in `Drop` when initialisation succeeded.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        ComScope(hr.is_ok())
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: matches the successful `CoInitializeEx` above.
            unsafe { CoUninitialize() };
        }
    }
}

/// Register the Vietnamese keyboard profile and the TSF categories.
pub fn register_profile(dll_path: &str) -> Result<()> {
    let _com = ComScope::new();
    // SAFETY: COM calls with valid arguments; interface pointers are released on drop.
    unsafe {
        let mgr: ITfInputProcessorProfileMgr =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
        let desc: Vec<u16> = PROFILE_DESCRIPTION.encode_utf16().collect();
        // The DLL carries the icon as resource 0 once the installer adds one; the empty icon file
        // makes Windows use a default.
        let icon: Vec<u16> = dll_path.encode_utf16().collect();
        mgr.RegisterProfile(
            &CLSID_PKEY,
            LANGID_VIETNAMESE,
            &GUID_PROFILE_VIETNAMESE,
            &desc,
            &icon,
            0,
            HKL::default(),
            0,
            false,
            0,
        )?;

        let cat: ITfCategoryMgr =
            CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)?;
        for c in [
            GUID_TFCAT_TIP_KEYBOARD,
            GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
            GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
            GUID_TFCAT_TIPCAP_UIELEMENTENABLED,
            GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
        ] {
            cat.RegisterCategory(&CLSID_PKEY, &c, &CLSID_PKEY)?;
        }
    }
    Ok(())
}

pub fn unregister_profile() -> Result<()> {
    let _com = ComScope::new();
    // SAFETY: as in `register_profile`.
    unsafe {
        let cat: ITfCategoryMgr =
            CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)?;
        for c in [
            GUID_TFCAT_TIP_KEYBOARD,
            GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
            GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
            GUID_TFCAT_TIPCAP_UIELEMENTENABLED,
            GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
        ] {
            let _ = cat.UnregisterCategory(&CLSID_PKEY, &c, &CLSID_PKEY);
        }
        let mgr: ITfInputProcessorProfileMgr =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
        mgr.UnregisterProfile(&CLSID_PKEY, LANGID_VIETNAMESE, &GUID_PROFILE_VIETNAMESE, 0)
            .or_else(|e| if e.code() == E_FAIL { Ok(()) } else { Err(e) })
    }
}
