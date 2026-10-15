//! End to end: the real app (hooks, engine, `SendInput`) types into a real Windows edit control.
//!
//! This sends actual keystrokes, so it only runs when `PKEY_RUN_E2E=1`, and it refuses to
//! send anything unless its own window really has the keyboard focus (so a click elsewhere
//! cannot make it type into another program). It needs an interactive desktop, and no other copy
//! of the app running. Run it with:
//!
//! `PKEY_RUN_E2E=1 cargo test -p pkey-app --features test-hooks --test e2e -- --nocapture`
#![cfg(windows)]

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, SetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowTextW,
    PostMessageW, SendMessageW, SetForegroundWindow, SetWindowTextW, ShowWindow, TranslateMessage,
    MSG, SW_SHOW, WINDOW_EX_STYLE, WM_CLOSE, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

const ES_MULTILINE: u32 = 0x0004;
const EM_SETSEL: u32 = 0x00B1;

fn type_key(vk: u32, up: bool) {
    let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC) } as u16;
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk as u16),
                wScan: scan,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: pkey_app::win::TEST_MAGIC,
            },
        },
    };
    unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
}

fn press(vk: u32) {
    type_key(vk, false);
    type_key(vk, true);
    thread::sleep(Duration::from_millis(25));
}

fn type_text(s: &str) {
    for c in s.chars() {
        press(if c == ' ' {
            0x20
        } else {
            c.to_ascii_uppercase() as u32
        });
    }
    // Let the hook, the injected edits and the control catch up.
    thread::sleep(Duration::from_millis(250));
}

fn toggle() {
    for (vk, up) in [(0x11, false), (0x10, false), (0x10, true), (0x11, true)] {
        type_key(vk, up);
        thread::sleep(Duration::from_millis(25));
    }
    thread::sleep(Duration::from_millis(150));
}

struct EditWindow {
    hwnd: HWND,
    pump: Option<thread::JoinHandle<()>>,
}

impl EditWindow {
    fn text(&self) -> String {
        let mut buf = [0u16; 512];
        let n = unsafe { GetWindowTextW(self.hwnd, &mut buf) } as usize;
        String::from_utf16_lossy(&buf[..n])
    }

    fn clear(&self) {
        unsafe {
            let _ = SetWindowTextW(self.hwnd, w!(""));
            SendMessageW(
                self.hwnd,
                EM_SETSEL,
                Some(WPARAM(usize::MAX)),
                Some(LPARAM(-1)),
            );
        }
    }
}

/// A top-level edit control with the focus, on its own message-pumping thread.
fn open_edit_window() -> Option<EditWindow> {
    let (tx, rx) = mpsc::channel();
    let pump = thread::spawn(move || unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            w!(""),
            WS_OVERLAPPEDWINDOW
                | WS_VISIBLE
                | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(ES_MULTILINE),
            200,
            200,
            600,
            200,
            None,
            None,
            None,
            None,
        );
        let Ok(hwnd) = hwnd else {
            let _ = tx.send(0isize);
            return;
        };
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        let _ = tx.send(hwnd.0 as isize);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            // Closing a plain window does not end the loop by itself.
            if !windows::Win32::UI::WindowsAndMessaging::IsWindow(Some(hwnd)).as_bool() {
                break;
            }
        }
    });
    let raw = rx.recv_timeout(Duration::from_secs(5)).ok()?;
    if raw == 0 {
        return None;
    }
    let hwnd = HWND(raw as *mut core::ffi::c_void);
    // Only go on if the window really has the keyboard.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if unsafe { GetForegroundWindow() } == hwnd {
            return Some(EditWindow {
                hwnd,
                pump: Some(pump),
            });
        }
        thread::sleep(Duration::from_millis(50));
    }
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    None
}

impl Drop for EditWindow {
    fn drop(&mut self) {
        unsafe {
            // Owned by the pump thread, which destroys it on WM_CLOSE.
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if let Some(p) = self.pump.take() {
            let _ = p.join();
        }
    }
}

#[test]
fn the_real_app_types_vietnamese_into_a_real_edit_control() {
    if std::env::var_os("PKEY_RUN_E2E").is_none() {
        eprintln!("SKIPPED: set PKEY_RUN_E2E=1 to type into a real window");
        return;
    }
    // The app holds a global keyboard hook; never leave it installed if something hangs.
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(40));
        eprintln!("e2e watchdog: giving up");
        std::process::exit(3);
    });
    let dir = std::env::temp_dir().join(format!("pkey-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("PKEY_CONFIG_DIR", &dir);

    // The app on its own thread, with its hooks.
    let (tx, rx) = mpsc::channel();
    let app = thread::spawn(move || pkey_app::win::run_with(false, Some(tx)));
    let Ok(app_hwnd) = rx.recv_timeout(Duration::from_secs(5)) else {
        eprintln!("SKIPPED: the app did not start (is another copy of pKey running?)");
        let _ = app.join();
        return;
    };

    let result = std::panic::catch_unwind(|| {
        let Some(win) = open_edit_window() else {
            eprintln!("SKIPPED: the test window could not take the keyboard focus");
            return;
        };
        eprintln!("e2e: window ready");
        win.clear();

        type_text("tieengs vieejt ");
        eprintln!("e2e: typed first line -> {:?}", win.text());
        assert_eq!(win.text(), "tiếng việt ");

        eprintln!("e2e: clearing");
        win.clear();
        eprintln!("e2e: cleared; typing tooi");
        type_text("tooi");
        eprintln!(
            "e2e: typed -> {:?}",
            win.text().escape_unicode().to_string().len()
        );
        press(0x08);
        eprintln!("e2e: backspace sent"); // Backspace edits the word, not what is before it
        type_text(" ");
        assert_eq!(win.text(), "tô ");

        win.clear();
        type_text("windows ");
        assert_eq!(
            win.text(),
            "windows ",
            "a word that is not Vietnamese is restored"
        );

        win.clear();
        toggle();
        type_text("tieengs ");
        assert_eq!(win.text(), "tieengs ", "English mode leaves the keys alone");
        toggle();
        type_text("tieengs ");
        assert_eq!(win.text(), "tieengs tiếng ");
    });

    unsafe {
        let _ = PostMessageW(
            Some(HWND(app_hwnd as *mut core::ffi::c_void)),
            WM_CLOSE,
            WPARAM(0),
            LPARAM(0),
        );
    }
    let _ = app.join();
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// The shipped file is a 32-bit program. This starts that real `.exe` as its own process (built
/// with `--features test-hooks`) and types into a window of *this* test process, which is 64-bit
/// on a normal run, to prove the hook and the injected keys work across bitness.
///
/// `PKEY_RUN_E2E=1 PKEY_E2E_EXE=<path to the test-hooks pkey.exe> cargo test -p pkey-app --test e2e`
#[test]
fn a_separate_pkey_exe_types_into_this_process() {
    let (Some(_), Some(exe)) = (
        std::env::var_os("PKEY_RUN_E2E"),
        std::env::var_os("PKEY_E2E_EXE"),
    ) else {
        eprintln!("SKIPPED: set PKEY_RUN_E2E=1 and PKEY_E2E_EXE=<test-hooks pkey.exe>");
        return;
    };
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(40));
        eprintln!("e2e watchdog: giving up");
        std::process::exit(3);
    });
    let dir = std::env::temp_dir().join(format!("pkey-e2e-exe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    struct Kill(std::process::Child, std::path::PathBuf);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let child = std::process::Command::new(&exe)
        .env("PKEY_CONFIG_DIR", &dir)
        .spawn()
        .expect("start pkey.exe");
    let _guard = Kill(child, dir);

    // Wait until the program's window exists, i.e. its hooks are installed.
    let start = Instant::now();
    while unsafe { windows::Win32::UI::WindowsAndMessaging::FindWindowW(w!("pKey.Tray"), None) }
        .is_err()
    {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "pkey.exe did not start"
        );
        thread::sleep(Duration::from_millis(50));
    }
    thread::sleep(Duration::from_millis(300));

    let Some(win) = open_edit_window() else {
        eprintln!("SKIPPED: the test window could not take the keyboard focus");
        return;
    };
    win.clear();
    type_text("tieengs vieejt ");
    assert_eq!(win.text(), "tiếng việt ");
    win.clear();
    type_text("windows ");
    assert_eq!(win.text(), "windows ");
    eprintln!(
        "cross-bitness OK: {}-bit test process, exe {:?}",
        if cfg!(target_pointer_width = "64") {
            64
        } else {
            32
        },
        exe
    );
}
