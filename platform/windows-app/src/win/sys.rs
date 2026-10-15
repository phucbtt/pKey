//! Questions the app asks Windows: who has the focus, what a key types, auto-start.

use std::collections::HashMap;

use ime_runtime::ime_config::AppIdentity;
use ime_runtime::ime_core::Modifiers;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, ToUnicodeEx,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongPtrW,
    GetWindowThreadProcessId, GUITHREADINFO, GWL_STYLE,
};

use crate::controller::Foreground;

const ES_PASSWORD: u32 = 0x20;
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "pKey";
/// The name used before the app was called pKey; still recognised and replaced.
const LEGACY_RUN_VALUE: &str = "VietIME";

pub fn exe_path_of(pid: u32) -> Option<String> {
    // SAFETY: plain Win32 calls; the handle is closed before returning.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize * 2];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// Process-name cache: asking Windows for the image path on every key would be wasteful.
#[derive(Default)]
pub struct IdentityCache {
    by_pid: HashMap<u32, AppIdentity>,
}

impl IdentityCache {
    fn identity(&mut self, pid: u32) -> AppIdentity {
        if self.by_pid.len() > 256 {
            self.by_pid.clear();
        }
        self.by_pid
            .entry(pid)
            .or_insert_with(|| {
                let path = exe_path_of(pid);
                AppIdentity {
                    exe_name: path
                        .as_deref()
                        .and_then(|p| p.rsplit(['\\', '/']).next())
                        .map(str::to_string),
                    process_path: path,
                    ..AppIdentity::default()
                }
            })
            .clone()
    }

    /// The focus window and whatever the app needs to know about it. `None` when there is no
    /// foreground window (secure desktop, locked screen).
    pub fn foreground(&mut self) -> Option<(Foreground, u32)> {
        // SAFETY: plain Win32 queries.
        unsafe {
            let top: HWND = GetForegroundWindow();
            if top.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            let thread = GetWindowThreadProcessId(top, Some(&mut pid));
            let mut info = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            let focus =
                if GetGUIThreadInfo(thread, &mut info).is_ok() && !info.hwndFocus.0.is_null() {
                    info.hwndFocus
                } else {
                    top
                };
            Some((
                Foreground {
                    window: focus.0 as usize as u64,
                    app: self.identity(pid),
                    secure: is_password_edit(focus),
                },
                thread,
            ))
        }
    }
}

/// A classic Win32 password box. Browsers and UWP apps mark passwords differently (UI
/// Automation), which this app does not read: see the limitations in the spec.
fn is_password_edit(hwnd: HWND) -> bool {
    // SAFETY: plain Win32 queries on a window handle.
    unsafe {
        let mut class = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut class) as usize;
        let name = String::from_utf16_lossy(&class[..n.min(class.len())]);
        (name.eq_ignore_ascii_case("Edit") || name.starts_with("RichEdit"))
            && GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & ES_PASSWORD != 0
    }
}

pub fn current_modifiers() -> Modifiers {
    let down = |vk: i32| unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0;
    let mut m = Modifiers::empty();
    if down(0x10) {
        m |= Modifiers::SHIFT;
    }
    if down(0x11) {
        m |= Modifiers::CTRL;
    }
    if down(0x12) {
        m |= Modifiers::ALT;
    }
    if down(0x5B) || down(0x5C) {
        m |= Modifiers::META;
    }
    // SAFETY: plain Win32 query; the low bit is the toggle state.
    if unsafe { GetKeyState(0x14) } & 1 != 0 {
        m |= Modifiers::CAPS_LOCK;
    }
    if down(0xA5) && down(0xA2) {
        m |= Modifiers::ALT_GR;
    }
    m
}

/// The character a key press types with the layout of the thread that has the focus.
pub fn translate_char(vk: u32, scan: u32, thread: u32) -> Option<char> {
    let mods = current_modifiers();
    let mut state = [0u8; 256];
    let mut set = |vk: usize, on: bool| {
        if on {
            state[vk] = 0x80;
        }
    };
    set(0x10, mods.contains(Modifiers::SHIFT));
    set(0x11, mods.contains(Modifiers::CTRL));
    set(0x12, mods.contains(Modifiers::ALT));
    if mods.contains(Modifiers::CAPS_LOCK) {
        state[0x14] = 1;
    }
    let mut buf = [0u16; 8];
    // SAFETY: valid buffers; flag 0x4 keeps the call from changing the dead-key state.
    let n = unsafe {
        let hkl = GetKeyboardLayout(thread);
        ToUnicodeEx(vk, scan, &state, &mut buf, 0x4, Some(hkl))
    };
    if n == 1 {
        char::from_u32(u32::from(buf[0])).filter(|c| !c.is_control())
    } else {
        None
    }
}

// ------------------------------------------------------------------ auto-start

fn run_key(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    // SAFETY: plain registry call with an owned string.
    let e = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            access,
            None,
            &mut key,
            None,
        )
    };
    e.is_ok().then_some(key)
}

pub fn autostart_enabled() -> bool {
    let Some(key) = run_key(KEY_READ) else {
        return false;
    };
    // SAFETY: querying only the size of the value.
    let found = unsafe {
        [RUN_VALUE, LEGACY_RUN_VALUE]
            .iter()
            .any(|n| RegQueryValueExW(key, &HSTRING::from(*n), None, None, None, None).is_ok())
    };
    // SAFETY: the key was opened above.
    unsafe {
        let _ = RegCloseKey(key);
    }
    found
}

pub fn set_autostart(on: bool) -> bool {
    let Some(key) = run_key(KEY_WRITE) else {
        return false;
    };
    // SAFETY: registry calls with owned data; the key is closed afterwards.
    let ok = unsafe {
        let name = HSTRING::from(RUN_VALUE);
        // Whatever happens next, the old entry goes: it points at the old program name.
        let _ = RegDeleteValueW(key, &HSTRING::from(LEGACY_RUN_VALUE));
        let r = if on {
            let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned());
            match exe {
                Ok(p) => {
                    let data: Vec<u16> = format!("\"{p}\"").encode_utf16().chain([0]).collect();
                    let bytes =
                        std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2);
                    RegSetValueExW(key, &name, None, REG_SZ, Some(bytes)).is_ok()
                }
                Err(_) => false,
            }
        } else {
            let e = RegDeleteValueW(key, &name);
            e.is_ok() || e.0 == 2 // already absent
        };
        let _ = RegCloseKey(key);
        r
    };
    ok
}
