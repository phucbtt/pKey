//! Loads the built DLL like COM would and checks the exports and the objects it hands out.
//! Nothing here touches the registry or registers an input method.
#![cfg(windows)]

use std::path::PathBuf;
use std::process::Command;

use windows::core::{s, Interface, GUID, HRESULT, PCSTR, PCWSTR};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, E_INVALIDARG, S_FALSE, S_OK};
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, IClassFactory, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::TextServices::{
    ITfCompositionSink, ITfDisplayAttributeProvider, ITfKeyEventSink, ITfTextInputProcessor,
    ITfTextInputProcessorEx, ITfThreadMgrEventSink, TF_ATTR_INPUT, TF_DISPLAYATTRIBUTE,
    TF_LS_SOLID,
};

const CLSID_PKEY: GUID = GUID::from_u128(0x434E620C_D5E7_4347_B6BC_BE6BE00C417D);
const DISPLAY_ATTRIBUTE: GUID = GUID::from_u128(0x35B7A9A7_B56C_4713_AF01_977C81D4FE7B);

type GetClassObject =
    unsafe extern "system" fn(*const GUID, *const GUID, *mut *mut core::ffi::c_void) -> HRESULT;
type CanUnload = unsafe extern "system" fn() -> HRESULT;
type Install = unsafe extern "system" fn(i32, *const u16) -> HRESULT;

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
    cmd.args(["build", "-p", "pkey-tsf", "--lib"])
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

struct Com;
impl Com {
    fn new() -> Com {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).unwrap() };
        Com
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[test]
fn dll_exports_and_objects() {
    let path = build_dll();
    assert!(path.exists(), "{}", path.display());
    let wide: Vec<u16> = path.to_string_lossy().encode_utf16().chain([0]).collect();
    let _com = Com::new();
    unsafe {
        let module = LoadLibraryW(PCWSTR(wide.as_ptr())).expect("load DLL");

        for name in [
            "DllGetClassObject\0",
            "DllCanUnloadNow\0",
            "DllRegisterServer\0",
            "DllUnregisterServer\0",
            "DllInstall\0",
        ] {
            assert!(
                GetProcAddress(module, PCSTR(name.as_ptr())).is_some(),
                "missing export {name:?}"
            );
        }
        let get_class_object: GetClassObject =
            std::mem::transmute(GetProcAddress(module, s!("DllGetClassObject")).unwrap());
        let can_unload: CanUnload =
            std::mem::transmute(GetProcAddress(module, s!("DllCanUnloadNow")).unwrap());
        let install: Install =
            std::mem::transmute(GetProcAddress(module, s!("DllInstall")).unwrap());

        // Only our class id is served.
        let mut raw = std::ptr::null_mut();
        let other = GUID::from_u128(1);
        assert_eq!(
            get_class_object(&other, &IClassFactory::IID, &mut raw),
            CLASS_E_CLASSNOTAVAILABLE
        );
        assert!(raw.is_null());

        // A bad DllInstall argument is rejected before anything is written.
        let bad: Vec<u16> = "nonsense\0".encode_utf16().collect();
        assert_eq!(install(1, bad.as_ptr()), E_INVALIDARG);

        assert_eq!(can_unload(), S_OK, "nothing alive yet");

        let hr = get_class_object(&CLSID_PKEY, &IClassFactory::IID, &mut raw);
        assert_eq!(hr, S_OK);
        let factory = IClassFactory::from_raw(raw);
        assert_eq!(can_unload(), S_FALSE, "the factory is alive");

        let tip: ITfTextInputProcessorEx =
            factory.CreateInstance(None).expect("create text service");
        // Every interface TSF may ask the text service for.
        tip.cast::<ITfTextInputProcessor>()
            .expect("ITfTextInputProcessor");
        tip.cast::<ITfKeyEventSink>().expect("ITfKeyEventSink");
        tip.cast::<ITfThreadMgrEventSink>()
            .expect("ITfThreadMgrEventSink");
        tip.cast::<ITfCompositionSink>()
            .expect("ITfCompositionSink");
        let provider = tip
            .cast::<ITfDisplayAttributeProvider>()
            .expect("ITfDisplayAttributeProvider");

        // The underline attribute: one item, our guid, a solid single underline.
        let en = provider.EnumDisplayAttributeInfo().unwrap();
        let mut items = [None];
        let mut fetched = 0;
        en.Next(&mut items, &mut fetched).ok().unwrap();
        assert_eq!(fetched, 1);
        let info = items[0].take().unwrap();
        assert_eq!(info.GetGUID().unwrap(), DISPLAY_ATTRIBUTE);
        let mut attr = TF_DISPLAYATTRIBUTE::default();
        info.GetAttributeInfo(&mut attr).unwrap();
        assert_eq!(attr.lsStyle, TF_LS_SOLID);
        assert_eq!(attr.bAttr, TF_ATTR_INPUT);
        let mut again = [None];
        en.Next(&mut again, &mut fetched).ok().unwrap();
        assert_eq!(fetched, 0, "only one attribute");
        provider
            .GetDisplayAttributeInfo(&DISPLAY_ATTRIBUTE)
            .expect("lookup by guid");
        assert!(provider.GetDisplayAttributeInfo(&other).is_err());

        drop((en, info, provider, tip, factory));
        assert_eq!(can_unload(), S_OK, "everything released");
    }
}
