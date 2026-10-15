//! The Windows shell of the app: hidden window, tray icon, keyboard and mouse hooks.
//!
//! Everything runs on one thread (the one that pumps messages), which is also what a low-level
//! hook requires. The hook callbacks reach the app state through a thread-local `RefCell`; if the
//! state is already borrowed (a re-entrant call while a menu is being handled) the event simply
//! passes through to the application.

mod inject;
mod settings;
mod sys;
mod tray;

use std::cell::RefCell;

use ime_runtime::ime_core::InputMode;
use ime_runtime::Runtime;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow, DispatchMessageW,
    FindWindowW, GetMessageW, KillTimer, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetTimer, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    EVENT_SYSTEM_FOREGROUND, HHOOK, HICON, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, MSG,
    SW_SHOWNORMAL, WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOW_EX_STYLE, WINDOW_STYLE,
    WINEVENT_OUTOFCONTEXT, WM_COMMAND, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYUP, WM_TIMER, WNDCLASSW,
};

use crate::controller::{Controller, KbEvent};
use inject::Replay;
use tray::*;

const ERROR_ALREADY_EXISTS: u32 = 183;

/// Holds the "one copy at a time" mutex and gives it back on drop.
struct InstanceGuard(windows::Win32::Foundation::HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: the handle came from `CreateMutexW` and is closed exactly once.
        unsafe {
            let _ = windows::Win32::System::Threading::ReleaseMutex(self.0);
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
const SELFTEST_TIMER: usize = 1;
/// Posted to the hidden window to open Settings from places that must not do it themselves.
const WM_OPEN_SETTINGS: u32 = 0x8000 + 3;

/// `dwExtraInfo` that test builds accept on injected keys (see the `test-hooks` feature).
pub const TEST_MAGIC: usize = 0x5649_4554; // "VIET"

struct App {
    ctl: Controller,
    ids: sys::IdentityCache,
    hwnd: HWND,
    icon_vi: HICON,
    icon_en: HICON,
    shown_mode: InputMode,
    keyboard: HHOOK,
    mouse: HHOOK,
    focus: HWINEVENTHOOK,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| {
        let mut g = a.try_borrow_mut().ok()?;
        g.as_mut().map(f)
    })
}

impl App {
    fn icon(&self, mode: InputMode) -> HICON {
        match mode {
            InputMode::Vietnamese => self.icon_vi,
            InputMode::English => self.icon_en,
        }
    }

    fn refresh_icon(&mut self, mode: InputMode) {
        let paused = self.ctl.paused;
        // A paused app shows the English tile: it is not typing Vietnamese.
        let shown = if paused { InputMode::English } else { mode };
        self.shown_mode = mode;
        update(self.hwnd, self.icon(shown), tooltip(mode, paused));
    }

    fn menu_state(&self) -> MenuState {
        MenuState::new(
            self.ctl.runtime().config(),
            self.shown_mode,
            self.ctl.paused,
            sys::autostart_enabled(),
        )
    }

    fn config_dir(&self) -> Option<std::path::PathBuf> {
        self.ctl.runtime().paths().map(|p| p.config_dir.clone())
    }

    fn change(&mut self, f: impl FnOnce(&mut ime_runtime::ime_config::Config)) {
        // A failed save keeps the change for this session; there is nowhere to report it.
        let _ = self.ctl.runtime_mut().update_config(f);
    }
}

// ------------------------------------------------------------------ hooks

fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

fn on_key(wparam: WPARAM, lparam: LPARAM) -> bool {
    // SAFETY: for HC_ACTION the system passes a valid KBDLLHOOKSTRUCT in `lparam`.
    let k = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    // Our own injected edits (and anyone else's synthetic input) are never interpreted.
    if k.flags.0 & LLKHF_INJECTED.0 != 0
        && !(cfg!(feature = "test-hooks") && k.dwExtraInfo == TEST_MAGIC)
    {
        return false;
    }
    let up = matches!(wparam.0 as u32, 0x0101 | WM_SYSKEYUP); // WM_KEYUP | WM_SYSKEYUP
    with_app(|app| {
        let Some((fg, thread)) = app.ids.foreground() else {
            return false;
        };
        let vk = k.vkCode;
        let extended = k.flags.0 & LLKHF_EXTENDED.0 != 0;
        let text = if up || is_modifier(vk) {
            None
        } else {
            sys::translate_char(vk, k.scanCode, thread)
        };
        let ev = KbEvent {
            vk,
            scan: k.scanCode,
            extended,
            up,
            text,
            modifiers: sys::current_modifiers(),
        };
        let d = app.ctl.key(&ev, &fg);
        if let Some(m) = d.mode_changed {
            app.refresh_icon(m);
        }
        if d.hotkey == Some(ime_runtime::HotkeyAction::OpenSettings) {
            // Do not build a window inside the keyboard hook.
            // SAFETY: posting to our own window.
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(app.hwnd),
                    WM_OPEN_SETTINGS,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
        if !d.ops.is_empty() {
            let replay = Replay {
                vk,
                scan: k.scanCode,
                extended,
            };
            inject::send(&d.ops, replay);
        }
        d.swallow
    })
    .unwrap_or(false)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == 0 {
        // A panic must never unwind into the system.
        let swallow = std::panic::catch_unwind(|| on_key(wparam, lparam)).unwrap_or(false);
        if swallow {
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == 0
        && matches!(
            wparam.0 as u32,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN
        )
    {
        // The caret may have moved anywhere: forget the word being typed.
        let _ = std::panic::catch_unwind(|| with_app(|app| app.ctl.pointer_down()));
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn focus_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let _ = std::panic::catch_unwind(|| {
        with_app(|app| {
            if let Some((fg, _)) = app.ids.foreground() {
                let mode = app.ctl.focus(&fg);
                app.refresh_icon(mode);
            }
        })
    });
}

// ------------------------------------------------------------------ window

fn open(path: &std::path::Path) {
    // SAFETY: plain shell call with owned strings.
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(path.as_os_str()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Open a text file in Notepad. `.toml` has no program associated on a normal Windows, so the
/// default "open" would fail with "no application is associated".
fn open_in_notepad(path: &std::path::Path) {
    let arg = format!("\"{}\"", path.display());
    // SAFETY: plain shell call with owned strings.
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("notepad.exe"),
            &HSTRING::from(arg),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

fn open_settings(app: &App, page: usize) {
    let settings = crate::settings_model::Settings::from_config(
        app.ctl.runtime().config(),
        sys::autostart_enabled(),
    );
    settings::open(settings, app.ctl.runtime().macros().to_vec(), page);
}

fn handle_command(app: &mut App, cmd: u32) {
    use ime_runtime::ime_core::{Scheme, ToneStyle};
    match cmd {
        IDM_SETTINGS => open_settings(app, 0),
        IDM_TOGGLE => {
            let m = app.ctl.toggle_mode();
            app.refresh_icon(m);
        }
        IDM_TELEX => app.change(|c| c.general.scheme = Scheme::Telex),
        IDM_VNI => app.change(|c| c.general.scheme = Scheme::Vni),
        IDM_VIQR => app.change(|c| c.general.scheme = Scheme::Viqr),
        IDM_MODERN => app.change(|c| c.typing.tone_style = ToneStyle::Modern),
        IDM_TRADITIONAL => app.change(|c| c.typing.tone_style = ToneStyle::Traditional),
        IDM_SPELL => app.change(|c| c.typing.spell_check = !c.typing.spell_check),
        IDM_RESTORE => {
            app.change(|c| c.typing.auto_restore_invalid = !c.typing.auto_restore_invalid)
        }
        IDM_FREE_MARKING => app.change(|c| c.typing.free_marking = !c.typing.free_marking),
        IDM_MACROS => app.change(|c| c.macros.enabled = !c.macros.enabled),
        IDM_PAUSE => {
            app.ctl.paused = !app.ctl.paused;
            let m = app.shown_mode;
            app.refresh_icon(m);
        }
        IDM_AUTOSTART => {
            let on = !sys::autostart_enabled();
            if sys::set_autostart(on) {
                app.change(|c| c.general.run_at_login = on);
            }
        }
        IDM_OPEN_DIR => {
            if let Some(d) = app.config_dir() {
                // The folder does not exist until something has been saved.
                let _ = std::fs::create_dir_all(&d);
                open(&d);
            }
        }
        IDM_OPEN_CONFIG => {
            if let Some(d) = app.config_dir() {
                let _ = std::fs::create_dir_all(&d);
                let file = d.join("config.toml");
                if !file.exists() {
                    // Create it from the current settings so there is something to edit.
                    app.change(|_| {});
                }
                open_in_notepad(&file);
            }
        }
        IDM_EXIT => {
            // SAFETY: destroying our own window.
            unsafe {
                let _ = DestroyWindow(app.hwnd);
            }
        }
        _ => {}
    }
}

fn taskbar_created() -> u32 {
    // SAFETY: plain call with a static string.
    unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY => {
            match lparam.0 as u32 {
                WM_LBUTTONDBLCLK => {
                    with_app(|app| open_settings(app, 0));
                }
                WM_LBUTTONUP => {
                    with_app(|app| {
                        let m = app.ctl.toggle_mode();
                        app.refresh_icon(m);
                    });
                }
                WM_RBUTTONUP => {
                    // Build the state first and show the menu without holding the borrow, so
                    // keys typed while it is open still reach the hook.
                    if let Some(state) = with_app(|app| app.menu_state()) {
                        let cmd = show_menu(hwnd, &state);
                        if cmd != 0 {
                            with_app(|app| handle_command(app, cmd));
                        }
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_OPEN_SETTINGS => {
            with_app(|app| open_settings(app, wparam.0));
            LRESULT(0)
        }
        WM_COMMAND => LRESULT(0),
        WM_TIMER if wparam.0 == SELFTEST_TIMER => {
            let _ = KillTimer(Some(hwnd), SELFTEST_TIMER);
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            tray::remove(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        m if m == taskbar_created() && m != 0 => {
            // Explorer restarted: put the icon back.
            with_app(|app| {
                let icon = app.icon(app.shown_mode);
                tray::add(hwnd, icon, tooltip(app.shown_mode, app.ctl.paused));
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// `--selftest`: set everything up, then quit after a moment. For checking the build.
pub fn run() {
    let selftest = std::env::args().any(|a| a == "--selftest");
    run_with(selftest, None);
}

/// The whole app. `ready` receives the handle of the hidden window once the hooks are in; posting
/// `WM_CLOSE` to it ends the app (used by tests).
pub fn run_with(selftest: bool, ready: Option<std::sync::mpsc::Sender<isize>>) {
    // SAFETY: standard Win32 start-up; every handle is released on the way out.
    unsafe {
        // One copy at a time.
        let guard = CreateMutexW(None, true, w!("Local\\pKey.App"));
        let already = windows::Win32::Foundation::GetLastError().0 == ERROR_ALREADY_EXISTS;
        // Released when this function returns, so a second start in the same process (a test) is
        // not mistaken for a second copy.
        let _guard = guard.clone().ok().map(InstanceGuard);
        if guard.is_err() || already {
            if let Ok(existing) = FindWindowW(w!("pKey.Tray"), None) {
                if !existing.0.is_null() {
                    let page = std::env::args()
                        .find(|a| a.starts_with("--settings"))
                        .and_then(|a| a.strip_prefix("--settings=").and_then(|n| n.parse().ok()))
                        .unwrap_or(0);
                    let _ = PostMessageW(Some(existing), WM_OPEN_SETTINGS, WPARAM(page), LPARAM(0));
                }
            }
            return;
        }
        let module = GetModuleHandleW(None).unwrap_or_default();
        let instance = HINSTANCE(module.0);
        let class = w!("pKey.Tray");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        });
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("pKey"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        ) else {
            return;
        };
        let (Some(icon_vi), Some(icon_en)) = (
            make_icon("VI", (225, 29, 72)),
            make_icon("EN", (37, 99, 235)),
        ) else {
            return;
        };

        let mut ctl = Controller::new(Runtime::from_default_paths());
        let mode = ctl.runtime().global_mode();
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(instance), 0);
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(instance), 0);
        let focus = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(focus_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let (Ok(keyboard), Ok(mouse)) = (keyboard, mouse) else {
            return;
        };
        ctl.paused = false;
        let first = if mode == InputMode::Vietnamese {
            icon_vi
        } else {
            icon_en
        };
        tray::add(hwnd, first, tooltip(mode, false));
        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                ctl,
                ids: sys::IdentityCache::default(),
                hwnd,
                icon_vi,
                icon_en,
                shown_mode: mode,
                keyboard,
                mouse,
                focus,
            });
        });
        if selftest {
            SetTimer(Some(hwnd), SELFTEST_TIMER, 700, None);
            println!("selftest: hooks installed, tray added");
        }
        // `--settings` or `--settings=N` opens the Settings window (on tab N) at start-up.
        if let Some(arg) = std::env::args().find(|a| a.starts_with("--settings")) {
            let page = arg
                .strip_prefix("--settings=")
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(hwnd),
                WM_OPEN_SETTINGS,
                WPARAM(page),
                LPARAM(0),
            );
        }
        if let Some(tx) = ready {
            let _ = tx.send(hwnd.0 as isize);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if settings::is_dialog_message(&msg) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        APP.with(|a| {
            if let Some(app) = a.borrow_mut().take() {
                let _ = UnhookWindowsHookEx(app.keyboard);
                let _ = UnhookWindowsHookEx(app.mouse);
                let _ = UnhookWinEvent(app.focus);
                let _ = DestroyIcon(app.icon_vi);
                let _ = DestroyIcon(app.icon_en);
            }
        });
        if selftest {
            println!("selftest: clean exit");
        }
    }
}
