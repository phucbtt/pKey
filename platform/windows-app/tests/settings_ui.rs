//! Drives the real Settings window by clicking its controls, then checks what was saved.
//! Opens a visible window (but sends no keystrokes); runs only with `PKEY_RUN_E2E=1`:
//! `PKEY_RUN_E2E=1 cargo test -p pkey-app --features test-hooks --test settings_ui -- --nocapture`
#![cfg(windows)]

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetDlgItem, PostMessageW, SendMessageW, SetWindowTextW, WM_CLOSE,
};

const WM_OPEN_SETTINGS: u32 = 0x8000 + 3;
const BM_CLICK: u32 = 0xF5;
const BM_GETCHECK: u32 = 0xF0;
const CB_SETCURSEL: u32 = 0x14E;
const WM_COMMAND: u32 = 0x111;

// Control ids (see `settings.rs`).
const ID_OK: i32 = 1;
const ID_VNI: i32 = 102;
const ID_TONE_OLD: i32 = 106;
const ID_SPELL: i32 = 110;
const ID_CTRL: i32 = 130;
const ID_SHIFT: i32 = 131;
const ID_ALT: i32 = 132;
const ID_KEY: i32 = 134;
const ID_MACRO_TRIGGER: i32 = 141;
const ID_MACRO_TEXT: i32 = 142;
const ID_MACRO_ADD: i32 = 143;
const ID_EXCL_EDIT: i32 = 151;
const ID_EXCL_ADD: i32 = 152;

fn find_settings() -> Option<HWND> {
    unsafe {
        FindWindowW(w!("pKey.Settings"), None)
            .ok()
            .filter(|h| !h.0.is_null())
    }
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if ok() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

fn ctl(win: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(Some(win), id).expect("control exists") }
}

fn click(win: HWND, id: i32) {
    unsafe { SendMessageW(ctl(win, id), BM_CLICK, Some(WPARAM(0)), Some(LPARAM(0))) };
}

fn checked(win: HWND, id: i32) -> bool {
    unsafe { SendMessageW(ctl(win, id), BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0 == 1 }
}

fn set_text(win: HWND, id: i32, s: &str) {
    unsafe {
        let _ = SetWindowTextW(ctl(win, id), &HSTRING::from(s));
    }
}

#[test]
fn clicking_through_the_settings_window_saves_the_configuration() {
    if std::env::var_os("PKEY_RUN_E2E").is_none() {
        eprintln!("SKIPPED: set PKEY_RUN_E2E=1 to open the Settings window");
        return;
    }
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(40));
        eprintln!("settings_ui watchdog: giving up");
        std::process::exit(3);
    });
    let dir = std::env::temp_dir().join(format!("pkey-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("PKEY_CONFIG_DIR", &dir);

    let (tx, rx) = mpsc::channel();
    let app = thread::spawn(move || pkey_app::win::run_with(false, Some(tx)));
    let Ok(raw) = rx.recv_timeout(Duration::from_secs(5)) else {
        eprintln!("SKIPPED: the app did not start (another copy running?)");
        return;
    };
    let app_hwnd = HWND(raw as *mut core::ffi::c_void);

    let result = std::panic::catch_unwind(|| {
        unsafe {
            let _ = PostMessageW(Some(app_hwnd), WM_OPEN_SETTINGS, WPARAM(0), LPARAM(0));
        }
        wait_for("the settings window", || find_settings().is_some());
        let win = find_settings().unwrap();
        // It starts with the defaults.
        assert!(checked(win, ID_SPELL));
        assert!(checked(win, ID_CTRL) && checked(win, ID_SHIFT) && !checked(win, ID_ALT));

        // Click through some options.
        click(win, ID_VNI);
        click(win, ID_TONE_OLD);
        click(win, ID_SPELL); // off
        click(win, ID_CTRL); // off -> hotkey becomes Shift + key
        click(win, ID_ALT); // on
        click(win, ID_SHIFT); // off -> Alt only ...
                              // ... plus the key "Z": index 0 is "none", 1 = Space, 2 = A ... 27 = Z
        unsafe {
            SendMessageW(
                ctl(win, ID_KEY),
                CB_SETCURSEL,
                Some(WPARAM(27)),
                Some(LPARAM(0)),
            );
        }

        // A macro and an excluded program.
        set_text(win, ID_MACRO_TRIGGER, "vn");
        set_text(win, ID_MACRO_TEXT, "Việt Nam");
        click(win, ID_MACRO_ADD);
        set_text(win, ID_EXCL_EDIT, "Game");
        click(win, ID_EXCL_ADD);

        click(win, ID_OK);
        wait_for("the window to close", || find_settings().is_none());

        let config = std::fs::read_to_string(dir.join("config.toml")).expect("config.toml written");
        assert!(config.contains("scheme = \"vni\""), "{config}");
        assert!(config.contains("tone_style = \"traditional\""), "{config}");
        assert!(config.contains("spell_check = false"), "{config}");
        assert!(config.contains("toggle_mode = \"Alt+Z\""), "{config}");
        assert!(config.contains("Tắt pKey: Game.exe"), "{config}");
        let macros = std::fs::read_to_string(dir.join("macros.json")).expect("macros.json written");
        assert!(
            macros.contains("\"trigger\": \"vn\"") && macros.contains("Việt Nam"),
            "{macros}"
        );

        // Reopening shows what was saved.
        unsafe {
            let _ = PostMessageW(Some(app_hwnd), WM_OPEN_SETTINGS, WPARAM(0), LPARAM(0));
        }
        wait_for("the settings window again", || find_settings().is_some());
        let win = find_settings().unwrap();
        assert!(checked(win, ID_VNI) && checked(win, ID_TONE_OLD));
        assert!(!checked(win, ID_SPELL) && checked(win, ID_ALT) && !checked(win, ID_CTRL));
        // Cancel closes without saving anything new.
        unsafe {
            SendMessageW(
                win,
                WM_COMMAND,
                Some(WPARAM(2)), // IDCANCEL
                Some(LPARAM(0)),
            );
        }
        wait_for("cancel to close the window", || find_settings().is_none());
    });

    unsafe {
        let _ = PostMessageW(Some(app_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    let _ = app.join();
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// Open, visit every page, cancel: three times in a row, then open once more and press OK.
/// A crash anywhere kills the test process, which is what this is looking for.
#[test]
fn the_settings_window_survives_being_opened_and_closed_repeatedly() {
    if std::env::var_os("PKEY_RUN_E2E").is_none() {
        eprintln!("SKIPPED: set PKEY_RUN_E2E=1 to open the Settings window");
        return;
    }
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(40));
        eprintln!("settings_ui watchdog: giving up");
        std::process::exit(3);
    });
    let dir = std::env::temp_dir().join(format!("pkey-ui2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("PKEY_CONFIG_DIR", &dir);

    let (tx, rx) = mpsc::channel();
    let app = thread::spawn(move || pkey_app::win::run_with(false, Some(tx)));
    let Ok(raw) = rx.recv_timeout(Duration::from_secs(5)) else {
        eprintln!("SKIPPED: the app did not start (another copy running?)");
        return;
    };
    let app_hwnd = HWND(raw as *mut core::ffi::c_void);

    let result = std::panic::catch_unwind(|| {
        for round in 0..3 {
            unsafe {
                let _ = PostMessageW(Some(app_hwnd), WM_OPEN_SETTINGS, WPARAM(round), LPARAM(0));
            }
            wait_for("the settings window", || find_settings().is_some());
            let win = find_settings().unwrap();
            for id in 400..=404 {
                click(win, id);
                thread::sleep(Duration::from_millis(60));
            }
            unsafe { SendMessageW(win, WM_COMMAND, Some(WPARAM(2)), Some(LPARAM(0))) };
            wait_for("cancel", || find_settings().is_none());
        }
        unsafe {
            let _ = PostMessageW(Some(app_hwnd), WM_OPEN_SETTINGS, WPARAM(0), LPARAM(0));
        }
        wait_for("the settings window", || find_settings().is_some());
        click(find_settings().unwrap(), ID_OK);
        wait_for("OK", || find_settings().is_none());
    });

    unsafe {
        let _ = PostMessageW(Some(app_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    let _ = app.join();
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
