//! The Settings window: tabs for typing options, the hotkey, macros and excluded programs.
//!
//! Built from plain Win32 controls (no extra UI library). It runs on the same thread and message
//! loop as the tray, so the keyboard hook keeps working while it is open. Reading and writing the
//! configuration is done by [`crate::settings_model`]; this file only draws and collects.

use std::cell::RefCell;
use std::ffi::c_void;

use ime_runtime::ime_core::{Macro, Scheme, ToneStyle};
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse,
    EndPaint, FillRect, GetDC, GetDeviceCaps, InvalidateRect, LineTo, MoveToEx, ReleaseDC,
    RoundRect, ScreenToClient, SelectObject, SetBkColor, SetBkMode, SetTextColor,
    CLEARTYPE_QUALITY, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FW_BOLD,
    FW_NORMAL, HBRUSH, HDC, HFONT, HPEN, LOGPIXELSX, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, SetWindowTheme, DRAWITEMSTRUCT, ICC_TAB_CLASSES, INITCOMMONCONTROLSEX,
    MARGINS, MEASUREITEMSTRUCT, NMHDR, ODT_COMBOBOX,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow, DrawIconEx, GetClientRect,
    GetCursorPos, GetDlgItem, GetSystemMetrics, GetWindowTextW, IsDialogMessageW, LoadImageW,
    MessageBoxW, RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowTextW, ShowWindow,
    DI_NORMAL, HICON, HMENU, IMAGE_ICON, LR_DEFAULTCOLOR, MB_ICONWARNING, MB_OK, MSG, SM_CXICON,
    SM_CXSCREEN, SM_CXSMICON, SM_CYSCREEN, SW_HIDE, SW_MINIMIZE, SW_SHOW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_DRAWITEM, WM_ERASEBKGND, WM_MEASUREITEM,
    WM_NOTIFY, WM_PAINT, WM_SETFONT, WM_SETICON, WNDCLASSW,
};

use super::sys;
use super::with_app;
use crate::settings_model::{key_choices, normalize_exe, HotkeyChoice, Settings};

// Window and control styles (numeric: they are the same flags the C headers define).
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_GROUP: u32 = 0x0002_0000;
const WS_POPUP: u32 = 0x8000_0000;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_FRAMELESS_APP: u32 = WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX | WS_SYSMENU;
const WM_NCCALCSIZE: u32 = 0x0083;
const WM_NCHITTEST: u32 = 0x0084;
const ID_TITLE_MIN: i32 = 390;
const ID_TITLE_CLOSE: i32 = 391;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_EX_CONTROLPARENT: u32 = 0x0001_0000;
const BS_PUSHBUTTON: u32 = 0;
const BS_DEFPUSHBUTTON: u32 = 1;
const BS_AUTOCHECKBOX: u32 = 3;
const BS_AUTORADIOBUTTON: u32 = 9;
const CBS_DROPDOWNLIST: u32 = 3;
const CBS_OWNERDRAWFIXED: u32 = 0x0010;
const CBS_HASSTRINGS: u32 = 0x0200;
const ES_AUTOHSCROLL: u32 = 0x80;
const LBS_NOTIFY: u32 = 1;
const LBS_NOINTEGRALHEIGHT: u32 = 0x100;

const BM_GETCHECK: u32 = 0xF0;
const BM_SETCHECK: u32 = 0xF1;
const CB_ADDSTRING: u32 = 0x143;
const CB_GETCURSEL: u32 = 0x147;
const CB_GETLBTEXT: u32 = 0x148;
const CB_SETCURSEL: u32 = 0x14E;
const CB_SETITEMHEIGHT: u32 = 0x153;
const CB_GETDROPPEDSTATE: u32 = 0x157;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_PRINTCLIENT: u32 = 0x0318;
const CBN_SELCHANGE: u16 = 1;
const LB_ADDSTRING: u32 = 0x180;
const LB_GETCURSEL: u32 = 0x188;
const LB_RESETCONTENT: u32 = 0x184;
const TCM_GETCURSEL: u32 = 0x130B;
const TCN_SELCHANGE: u32 = -551i32 as u32;
const BN_CLICKED: u32 = 0;
const LBN_SELCHANGE: u32 = 1;

// Control ids.
const ID_OK: i32 = 1;
const ID_CANCEL: i32 = 2;
const ID_APPLY: i32 = 0x300;
const ID_TELEX: i32 = 101;
const ID_VNI: i32 = 102;
const ID_VIQR: i32 = 103;
const ID_ENCODING: i32 = 104;
const ID_TONE_NEW: i32 = 105;
const ID_TONE_OLD: i32 = 106;
const ID_SPELL: i32 = 110;
const ID_RESTORE: i32 = 111;
const ID_FREE: i32 = 112;
const ID_ESC: i32 = 113;
const ID_RAWBS: i32 = 114;
const ID_W: i32 = 115;
const ID_BRACKETS: i32 = 116;
const ID_FOREIGN: i32 = 117;
const ID_MACROS_ON: i32 = 118;
const ID_AUTOSTART: i32 = 119;
const ID_CTRL: i32 = 130;
const ID_SHIFT: i32 = 131;
const ID_ALT: i32 = 132;
const ID_WIN: i32 = 133;
const ID_KEY: i32 = 134;
const ID_MACRO_LIST: i32 = 140;
const ID_MACRO_TRIGGER: i32 = 141;
const ID_MACRO_TEXT: i32 = 142;
const ID_MACRO_ADD: i32 = 143;
const ID_MACRO_DEL: i32 = 144;
const ID_EXCL_LIST: i32 = 150;
const ID_EXCL_EDIT: i32 = 151;
const ID_EXCL_ADD: i32 = 152;
const ID_EXCL_DEL: i32 = 153;
const ID_TAB_BASE: i32 = 400;

const PAGES: [&str; 5] = [
    "Cài đặt chung",
    "Phím tắt",
    "Gõ tắt",
    "Ứng dụng loại trừ",
    "Thông tin",
];

const BM_GETSTATE: u32 = 0x00F2;
const BST_PUSHED: u32 = 0x0004;
const WM_GETFONT: u32 = 0x0031;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_SETFOCUS: u32 = 0x0007;
const WM_KILLFOCUS: u32 = 0x0008;

const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORLISTBOX: u32 = 0x0134;
const WM_CTLCOLORBTN: u32 = 0x0135;
const WM_CTLCOLORDLG: u32 = 0x0136;
const WM_CTLCOLORSTATIC: u32 = 0x0138;

struct Win {
    hwnd: HWND,
    tab: HWND,
    font: HFONT,
    bold_font: HFONT,
    title_font: HFONT,
    dpi: i32,
    current_page: usize,
    bg_brush: HBRUSH,
    sidebar_brush: HBRUSH,
    card_brush: HBRUSH,
    border_brush: HBRUSH,
    card_pen: HPEN,
    app_icon: Option<HICON>,
    controls: Vec<(HWND, usize)>,
    sidebar_buttons: Vec<HWND>,
    macros: Vec<Macro>,
    excluded: Vec<String>,
    keys: Vec<(String, u32)>,
}

thread_local! {
    static WIN: RefCell<Option<Win>> = const { RefCell::new(None) };
}

fn send(h: HWND, msg: u32, w: usize, l: isize) -> isize {
    // SAFETY: plain message to a control we own.
    unsafe { SendMessageW(h, msg, Some(WPARAM(w)), Some(LPARAM(l))).0 }
}

fn item(hwnd: HWND, id: i32) -> HWND {
    // SAFETY: plain lookup of a child control.
    unsafe { GetDlgItem(Some(hwnd), id).unwrap_or_default() }
}

fn text_of(h: HWND) -> String {
    let mut buf = [0u16; 2048];
    // SAFETY: valid buffer.
    let n = unsafe { GetWindowTextW(h, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n])
}

fn set_text(h: HWND, s: &str) {
    // SAFETY: owned string.
    unsafe {
        let _ = SetWindowTextW(h, &HSTRING::from(s));
    }
}

fn add_string(h: HWND, msg: u32, s: &str) {
    let ws = HSTRING::from(s);
    send(h, msg, 0, ws.as_ptr() as isize);
}

fn checked(hwnd: HWND, id: i32) -> bool {
    send(item(hwnd, id), BM_GETCHECK, 0, 0) == 1
}

fn set_checked(hwnd: HWND, id: i32, on: bool) {
    send(item(hwnd, id), BM_SETCHECK, usize::from(on), 0);
}

unsafe extern "system" fn checkbox_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = if msg == WM_PRINTCLIENT {
                HDC(wparam.0 as *mut c_void)
            } else {
                BeginPaint(hwnd, &mut ps)
            };
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            let is_checked =
                SendMessageW(hwnd, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0 == 1;

            // Background of card surface: pure white #FFFFFF
            let card_bg = CreateSolidBrush(COLORREF(0x00FF_FFFF));
            FillRect(hdc, &rc, card_bg);
            let _ = DeleteObject(card_bg.into());

            let box_sz = 16;
            let box_top = (rc.bottom - rc.top - box_sz) / 2;
            let box_left = 2;
            let box_rect = RECT {
                left: box_left,
                top: box_top,
                right: box_left + box_sz,
                bottom: box_top + box_sz,
            };

            if is_checked {
                // EVKey Electric Blue: #0D84FF (BGR: 0x00FF_840D)
                let blue_brush = CreateSolidBrush(COLORREF(0x00FF_840D));
                let blue_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00FF_840D));
                let old_b = SelectObject(hdc, blue_brush.into());
                let old_p = SelectObject(hdc, blue_pen.into());
                let _ = RoundRect(
                    hdc,
                    box_rect.left,
                    box_rect.top,
                    box_rect.right,
                    box_rect.bottom,
                    4,
                    4,
                );

                // Sharp white checkmark
                let white_pen = CreatePen(PS_SOLID, 2, COLORREF(0x00FF_FFFF));
                SelectObject(hdc, white_pen.into());
                let _ = MoveToEx(hdc, box_left + 3, box_top + 8, None);
                let _ = LineTo(hdc, box_left + 7, box_top + 12);
                let _ = LineTo(hdc, box_left + 13, box_top + 4);

                SelectObject(hdc, old_b);
                SelectObject(hdc, old_p);
                let _ = DeleteObject(blue_brush.into());
                let _ = DeleteObject(blue_pen.into());
                let _ = DeleteObject(white_pen.into());
            } else {
                // Unchecked: white surface with soft border #BAC3D0
                let white_brush = CreateSolidBrush(COLORREF(0x00FF_FFFF));
                let border_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00D0_C3BA));
                let old_b = SelectObject(hdc, white_brush.into());
                let old_p = SelectObject(hdc, border_pen.into());
                let _ = RoundRect(
                    hdc,
                    box_rect.left,
                    box_rect.top,
                    box_rect.right,
                    box_rect.bottom,
                    4,
                    4,
                );
                SelectObject(hdc, old_b);
                SelectObject(hdc, old_p);
                let _ = DeleteObject(white_brush.into());
                let _ = DeleteObject(border_pen.into());
            }

            // Draw label text in deep charcoal #1E232A
            let mut text = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut text) as usize;
            if len > 0 {
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x002A_231E));
                let hfont = SendMessageW(hwnd, WM_GETFONT, Some(WPARAM(0)), Some(LPARAM(0))).0;
                let old_font = if hfont != 0 {
                    SelectObject(hdc, HFONT(hfont as *mut _).into())
                } else {
                    windows::Win32::Graphics::Gdi::HGDIOBJ::default()
                };
                let mut text_rect = RECT {
                    left: box_left + box_sz + 8,
                    top: rc.top,
                    right: rc.right,
                    bottom: rc.bottom,
                };
                DrawTextW(
                    hdc,
                    &mut text[..len],
                    &mut text_rect,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );
                if hfont != 0 {
                    SelectObject(hdc, old_font);
                }
            }

            if msg != WM_PRINTCLIENT {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        BM_SETCHECK | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_SETFOCUS | WM_KILLFOCUS => {
            let res = DefSubclassProc(hwnd, msg, wparam, lparam);
            let _ = InvalidateRect(Some(hwnd), None, false);
            res
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn radio_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = if msg == WM_PRINTCLIENT {
                HDC(wparam.0 as *mut c_void)
            } else {
                BeginPaint(hwnd, &mut ps)
            };
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            let is_checked =
                SendMessageW(hwnd, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0 == 1;

            let card_bg = CreateSolidBrush(COLORREF(0x00FF_FFFF));
            FillRect(hdc, &rc, card_bg);
            let _ = DeleteObject(card_bg.into());

            let sz = 16;
            let top = (rc.bottom - rc.top - sz) / 2;
            let left = 2;

            if is_checked {
                // Outer ring: Electric Blue #0D84FF
                let ring_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00FF_840D));
                let white_brush = CreateSolidBrush(COLORREF(0x00FF_FFFF));
                let old_p = SelectObject(hdc, ring_pen.into());
                let old_b = SelectObject(hdc, white_brush.into());
                let _ = Ellipse(hdc, left, top, left + sz, top + sz);

                // Inner circle: solid filled #0D84FF
                let inner_brush = CreateSolidBrush(COLORREF(0x00FF_840D));
                let inner_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00FF_840D));
                SelectObject(hdc, inner_brush.into());
                SelectObject(hdc, inner_pen.into());
                let pad = 4;
                let _ = Ellipse(hdc, left + pad, top + pad, left + sz - pad, top + sz - pad);

                SelectObject(hdc, old_p);
                SelectObject(hdc, old_b);
                let _ = DeleteObject(ring_pen.into());
                let _ = DeleteObject(white_brush.into());
                let _ = DeleteObject(inner_brush.into());
                let _ = DeleteObject(inner_pen.into());
            } else {
                let border_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00D0_C3BA));
                let white_brush = CreateSolidBrush(COLORREF(0x00FF_FFFF));
                let old_p = SelectObject(hdc, border_pen.into());
                let old_b = SelectObject(hdc, white_brush.into());
                let _ = Ellipse(hdc, left, top, left + sz, top + sz);
                SelectObject(hdc, old_p);
                SelectObject(hdc, old_b);
                let _ = DeleteObject(border_pen.into());
                let _ = DeleteObject(white_brush.into());
            }

            // Draw label text in deep charcoal #1E232A
            let mut text = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut text) as usize;
            if len > 0 {
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x002A_231E));
                let hfont = SendMessageW(hwnd, WM_GETFONT, Some(WPARAM(0)), Some(LPARAM(0))).0;
                let old_font = if hfont != 0 {
                    SelectObject(hdc, HFONT(hfont as *mut _).into())
                } else {
                    windows::Win32::Graphics::Gdi::HGDIOBJ::default()
                };
                let mut text_rect = RECT {
                    left: left + sz + 8,
                    top: rc.top,
                    right: rc.right,
                    bottom: rc.bottom,
                };
                DrawTextW(
                    hdc,
                    &mut text[..len],
                    &mut text_rect,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );
                if hfont != 0 {
                    SelectObject(hdc, old_font);
                }
            }

            if msg != WM_PRINTCLIENT {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        BM_SETCHECK | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_SETFOCUS | WM_KILLFOCUS => {
            let res = DefSubclassProc(hwnd, msg, wparam, lparam);
            let _ = InvalidateRect(Some(hwnd), None, false);
            res
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn combobox_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = if msg == WM_PRINTCLIENT {
                HDC(wparam.0 as *mut c_void)
            } else {
                BeginPaint(hwnd, &mut ps)
            };
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            let is_dropped =
                SendMessageW(hwnd, CB_GETDROPPEDSTATE, Some(WPARAM(0)), Some(LPARAM(0))).0 != 0;
            let is_focused = GetFocus() == hwnd || is_dropped;

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = ScreenToClient(hwnd, &mut pt);
            let is_hovered =
                pt.x >= rc.left && pt.x < rc.right && pt.y >= rc.top && pt.y < rc.bottom;

            // 1. Draw rounded container (card white surface with subtle modern border)
            let bg_brush = CreateSolidBrush(COLORREF(0x00FF_FFFF));
            let border_col = if is_focused {
                0x00FF_840D // Electric Blue #0D84FF
            } else if is_hovered {
                0x00FF_B470 // Hover soft blue
            } else {
                0x00D0_C3BA // Soft muted gray-blue #BAC3D0
            };
            let border_pen = CreatePen(
                PS_SOLID,
                if is_focused { 2 } else { 1 },
                COLORREF(border_col),
            );
            let old_b = SelectObject(hdc, bg_brush.into());
            let old_p = SelectObject(hdc, border_pen.into());
            let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, 6, 6);
            SelectObject(hdc, old_b);
            SelectObject(hdc, old_p);
            let _ = DeleteObject(bg_brush.into());
            let _ = DeleteObject(border_pen.into());

            // 2. Modern Chevron Arrow on right
            let cy = (rc.top + rc.bottom) / 2;
            let arrow_x = rc.right - 14;
            let arrow_color = if is_focused {
                0x00FF_840D
            } else if is_hovered {
                0x00D0_6500
            } else {
                0x0069_5547
            };
            let arrow_pen = CreatePen(PS_SOLID, 2, COLORREF(arrow_color));
            let old_ap = SelectObject(hdc, arrow_pen.into());
            if is_dropped {
                let _ = MoveToEx(hdc, arrow_x - 4, cy + 2, None);
                let _ = LineTo(hdc, arrow_x, cy - 2);
                let _ = LineTo(hdc, arrow_x + 5, cy + 3);
            } else {
                let _ = MoveToEx(hdc, arrow_x - 4, cy - 2, None);
                let _ = LineTo(hdc, arrow_x, cy + 2);
                let _ = LineTo(hdc, arrow_x + 5, cy - 3);
            }
            SelectObject(hdc, old_ap);
            let _ = DeleteObject(arrow_pen.into());

            // 3. Draw current selected item text
            let sel = SendMessageW(hwnd, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0;
            if sel >= 0 {
                let mut text = [0u16; 256];
                let len = SendMessageW(
                    hwnd,
                    CB_GETLBTEXT,
                    Some(WPARAM(sel as usize)),
                    Some(LPARAM(text.as_mut_ptr() as isize)),
                )
                .0;
                if len > 0 {
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, COLORREF(0x002A_231E));
                    let hfont = SendMessageW(hwnd, WM_GETFONT, Some(WPARAM(0)), Some(LPARAM(0))).0;
                    let old_font = if hfont != 0 {
                        SelectObject(hdc, HFONT(hfont as *mut _).into())
                    } else {
                        windows::Win32::Graphics::Gdi::HGDIOBJ::default()
                    };
                    let mut text_rect = RECT {
                        left: rc.left + 10,
                        top: rc.top,
                        right: rc.right - 26,
                        bottom: rc.bottom,
                    };
                    DrawTextW(
                        hdc,
                        &mut text[..len as usize],
                        &mut text_rect,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                    );
                    if hfont != 0 {
                        SelectObject(hdc, old_font);
                    }
                }
            }

            if msg != WM_PRINTCLIENT {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);
            let _ = InvalidateRect(Some(hwnd), None, false);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        WM_MOUSELEAVE => {
            let _ = InvalidateRect(Some(hwnd), None, false);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        CB_SETCURSEL | WM_SETFOCUS | WM_KILLFOCUS | WM_LBUTTONDOWN | WM_LBUTTONUP => {
            let res = DefSubclassProc(hwnd, msg, wparam, lparam);
            let _ = InvalidateRect(Some(hwnd), None, false);
            res
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn button_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    is_primary: usize,
) -> LRESULT {
    match msg {
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = if msg == WM_PRINTCLIENT {
                HDC(wparam.0 as *mut c_void)
            } else {
                BeginPaint(hwnd, &mut ps)
            };
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            let is_pressed = (SendMessageW(hwnd, BM_GETSTATE, Some(WPARAM(0)), Some(LPARAM(0))).0
                & (BST_PUSHED as isize))
                != 0;

            let is_sidebar_tab = id >= ID_TAB_BASE as usize && id < (ID_TAB_BASE + 5) as usize;
            let is_active_tab = if is_sidebar_tab {
                WIN.with(|w| {
                    w.try_borrow()
                        .ok()
                        .and_then(|g| {
                            g.as_ref()
                                .map(|win| win.current_page == (id - ID_TAB_BASE as usize))
                        })
                        .unwrap_or(false)
                })
            } else {
                false
            };

            if is_sidebar_tab {
                // Sidebar background behind button #EAEEF4
                let bg_brush = CreateSolidBrush(COLORREF(0x00F4_EEEA));
                FillRect(hdc, &rc, bg_brush);
                let _ = DeleteObject(bg_brush.into());

                let (btn_bg, border_color, text_color, show_indicator) = if is_active_tab {
                    // Active tab: pure white pill with subtle border and blue indicator
                    (0x00FF_FFFF, 0x00E0_D7CF, 0x00D0_6500, true)
                } else if is_pressed {
                    (0x00DF_D7D0, 0x00DF_D7D0, 0x0047_3D33, false)
                } else {
                    (0x00F4_EEEA, 0x00F4_EEEA, 0x0069_5547, false)
                };

                let brush = CreateSolidBrush(COLORREF(btn_bg));
                let pen = CreatePen(PS_SOLID, 1, COLORREF(border_color));
                let old_b = SelectObject(hdc, brush.into());
                let old_p = SelectObject(hdc, pen.into());
                let _ = RoundRect(
                    hdc,
                    rc.left + 2,
                    rc.top + 1,
                    rc.right - 2,
                    rc.bottom - 1,
                    6,
                    6,
                );
                SelectObject(hdc, old_b);
                SelectObject(hdc, old_p);
                let _ = DeleteObject(brush.into());
                let _ = DeleteObject(pen.into());

                if show_indicator {
                    // Electric Blue 4px accent indicator
                    let ind_brush = CreateSolidBrush(COLORREF(0x00FF_840D));
                    let ind_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00FF_840D));
                    let old_b = SelectObject(hdc, ind_brush.into());
                    let old_p = SelectObject(hdc, ind_pen.into());
                    let _ = RoundRect(
                        hdc,
                        rc.left + 5,
                        rc.top + 6,
                        rc.left + 9,
                        rc.bottom - 6,
                        3,
                        3,
                    );
                    SelectObject(hdc, old_b);
                    SelectObject(hdc, old_p);
                    let _ = DeleteObject(ind_brush.into());
                    let _ = DeleteObject(ind_pen.into());
                }

                let mut text = [0u16; 512];
                let len = GetWindowTextW(hwnd, &mut text) as usize;
                if len > 0 {
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, COLORREF(text_color));
                    let hfont = WIN.with(|w| {
                        w.try_borrow().ok().and_then(|g| {
                            g.as_ref().map(|win| {
                                if is_active_tab {
                                    win.bold_font
                                } else {
                                    win.font
                                }
                            })
                        })
                    });
                    let old_font = if let Some(hf) = hfont {
                        SelectObject(hdc, hf.into())
                    } else {
                        windows::Win32::Graphics::Gdi::HGDIOBJ::default()
                    };
                    let mut text_rect = RECT {
                        left: rc.left + 12,
                        top: rc.top,
                        right: rc.right - 4,
                        bottom: rc.bottom,
                    };
                    DrawTextW(
                        hdc,
                        &mut text[..len],
                        &mut text_rect,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                    );
                    if hfont.is_some() {
                        SelectObject(hdc, old_font);
                    }
                }
            } else {
                // Action Buttons:
                let win_bg = CreateSolidBrush(COLORREF(0x00F8_F4F2));
                FillRect(hdc, &rc, win_bg);
                let _ = DeleteObject(win_bg.into());

                let (bg_col, border_col, text_col) = if is_primary == 1 {
                    if is_pressed {
                        (0x00CC_6600, 0x00FF_840D, 0x00FF_FFFF)
                    } else {
                        (0x00FF_840D, 0x00DA_6909, 0x00FF_FFFF)
                    }
                } else if id == ID_CANCEL as usize || id == ID_APPLY as usize {
                    if is_pressed {
                        (0x00E8_E4DF, 0x00CE_C7BE, 0x002F_2924)
                    } else {
                        (0x00FF_FFFF, 0x00DE_D7D0, 0x002F_2924)
                    }
                } else if id == ID_MACRO_DEL as usize || id == ID_EXCL_DEL as usize {
                    if is_pressed {
                        (0x00E5_E5FF, 0x00B0_AAFF, 0x002E_22CF)
                    } else {
                        (0x00F5_F5FF, 0x00D0_CCFF, 0x002E_22CF)
                    }
                } else {
                    if is_pressed {
                        (0x00E8_E4DF, 0x00CE_C7BE, 0x002F_2924)
                    } else {
                        (0x00FF_FFFF, 0x00DE_D7D0, 0x002F_2924)
                    }
                };

                let brush = CreateSolidBrush(COLORREF(bg_col));
                let pen = CreatePen(PS_SOLID, 1, COLORREF(border_col));
                let old_b = SelectObject(hdc, brush.into());
                let old_p = SelectObject(hdc, pen.into());
                let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, 6, 6);
                SelectObject(hdc, old_b);
                SelectObject(hdc, old_p);
                let _ = DeleteObject(brush.into());
                let _ = DeleteObject(pen.into());

                let mut text = [0u16; 512];
                let len = GetWindowTextW(hwnd, &mut text) as usize;
                if len > 0 {
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, COLORREF(text_col));
                    let hfont = WIN.with(|w| {
                        w.try_borrow().ok().and_then(|g| {
                            g.as_ref().map(|win| {
                                if is_primary == 1 {
                                    win.bold_font
                                } else {
                                    win.font
                                }
                            })
                        })
                    });
                    let old_font = if let Some(hf) = hfont {
                        SelectObject(hdc, hf.into())
                    } else {
                        windows::Win32::Graphics::Gdi::HGDIOBJ::default()
                    };
                    let mut text_rect = RECT {
                        left: rc.left,
                        top: rc.top + if is_pressed { 1 } else { 0 },
                        right: rc.right,
                        bottom: rc.bottom + if is_pressed { 1 } else { 0 },
                    };
                    DrawTextW(
                        hdc,
                        &mut text[..len],
                        &mut text_rect,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                    );
                    if hfont.is_some() {
                        SelectObject(hdc, old_font);
                    }
                }
            }

            if msg != WM_PRINTCLIENT {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_SETFOCUS | WM_KILLFOCUS => {
            let res = DefSubclassProc(hwnd, msg, wparam, lparam);
            let _ = InvalidateRect(Some(hwnd), None, false);
            res
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn title_button_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_PAINT | WM_PRINTCLIENT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = if msg == WM_PRINTCLIENT {
                HDC(wparam.0 as *mut c_void)
            } else {
                BeginPaint(hwnd, &mut ps)
            };
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = ScreenToClient(hwnd, &mut pt);
            let is_hovered =
                pt.x >= rc.left && pt.x < rc.right && pt.y >= rc.top && pt.y < rc.bottom;
            let is_pressed = SendMessageW(hwnd, BM_GETSTATE, Some(WPARAM(0)), Some(LPARAM(0))).0
                as u32
                & BST_PUSHED
                != 0;

            let is_close = id == ID_TITLE_CLOSE as usize;

            // Background
            let bg_col = if is_close {
                if is_pressed {
                    0x001B_0FBA // Dark Crimson Red
                } else if is_hovered {
                    0x0023_11E8 // Windows 11 Close Red #E81123
                } else {
                    0x00F8_F4F2 // Match content background
                }
            } else {
                if is_pressed {
                    0x00D0_C6BE
                } else if is_hovered {
                    0x00E4_DDD6
                } else {
                    0x00F8_F4F2
                }
            };

            let brush = CreateSolidBrush(COLORREF(bg_col));
            FillRect(hdc, &rc, brush);
            let _ = DeleteObject(brush.into());

            // Icon color
            let icon_col = if is_close && is_hovered {
                0x00FF_FFFF // Pure white cross when hovered
            } else {
                0x004A_3A2E // Deep charcoal glyph
            };
            let pen = CreatePen(PS_SOLID, 1, COLORREF(icon_col));
            let old_pen = SelectObject(hdc, pen.into());

            let cx = (rc.left + rc.right) / 2;
            let cy = (rc.top + rc.bottom) / 2;

            if is_close {
                // Draw ✕ icon (10px wide)
                let r = 5;
                let _ = MoveToEx(hdc, cx - r, cy - r, None);
                let _ = LineTo(hdc, cx + r + 1, cy + r + 1);
                let _ = MoveToEx(hdc, cx - r, cy + r, None);
                let _ = LineTo(hdc, cx + r + 1, cy - r - 1);
            } else {
                // Draw ━ minimize icon (10px wide line)
                let r = 5;
                let _ = MoveToEx(hdc, cx - r, cy, None);
                let _ = LineTo(hdc, cx + r + 1, cy);
            }

            SelectObject(hdc, old_pen);
            let _ = DeleteObject(pen.into());

            if msg != WM_PRINTCLIENT {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);
            let _ = InvalidateRect(Some(hwnd), None, false);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        WM_MOUSELEAVE => {
            let _ = InvalidateRect(Some(hwnd), None, false);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONDOWN | WM_LBUTTONUP => {
            let res = DefSubclassProc(hwnd, msg, wparam, lparam);
            let _ = InvalidateRect(Some(hwnd), None, false);
            res
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

impl Win {
    fn s(&self, v: i32) -> i32 {
        v * self.dpi / 96
    }

    #[allow(clippy::too_many_arguments)]
    fn ctl(
        &mut self,
        class: PCWSTR,
        text: &str,
        style: u32,
        (x, y, w, h): (i32, i32, i32, i32),
        id: i32,
        page: usize,
    ) -> HWND {
        // SAFETY: creating a child control of our own window.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                &HSTRING::from(text),
                WINDOW_STYLE(WS_CHILD | WS_VISIBLE | style),
                self.s(x),
                self.s(y),
                self.s(w),
                self.s(h),
                Some(self.hwnd),
                Some(HMENU(id as isize as *mut c_void)),
                None,
                None,
            )
            .unwrap_or_default()
        };
        send(hwnd, WM_SETFONT, self.font.0 as usize, 1);
        if class == w!("EDIT") || class == w!("LISTBOX") {
            unsafe {
                let _ = SetWindowTheme(hwnd, w!("Explorer"), None);
            }
        }
        self.controls.push((hwnd, page));
        hwnd
    }

    fn combobox(&mut self, (x, y, w, h): (i32, i32, i32, i32), id: i32, page: usize) -> HWND {
        let hwnd = self.ctl(
            w!("COMBOBOX"),
            "",
            CBS_DROPDOWNLIST | CBS_OWNERDRAWFIXED | CBS_HASSTRINGS | WS_TABSTOP | WS_VSCROLL,
            (x, y, w, h),
            id,
            page,
        );
        unsafe {
            let _ = SetWindowSubclass(hwnd, Some(combobox_subclass), id as usize, 0);
            let _ = SetWindowTheme(hwnd, w!("CFD"), None);
        }
        let item_h = self.s(28);
        send(hwnd, CB_SETITEMHEIGHT, usize::MAX, self.s(24) as isize);
        send(hwnd, CB_SETITEMHEIGHT, 0, item_h as isize);
        hwnd
    }

    fn label(&mut self, text: &str, rect: (i32, i32, i32, i32), page: usize) {
        self.ctl(w!("STATIC"), text, 0x80, rect, -1, page);
    }

    fn section_title(&mut self, text: &str, rect: (i32, i32, i32, i32), page: usize) {
        let hwnd = self.ctl(w!("STATIC"), text, 0x80, rect, -1, page);
        send(hwnd, WM_SETFONT, self.bold_font.0 as usize, 1);
    }

    fn check(&mut self, text: &str, id: i32, rect: (i32, i32, i32, i32), page: usize) {
        let hwnd = self.ctl(
            w!("BUTTON"),
            text,
            BS_AUTOCHECKBOX | WS_TABSTOP,
            rect,
            id,
            page,
        );
        unsafe {
            let _ = SetWindowSubclass(hwnd, Some(checkbox_subclass), id as usize, 0);
        }
    }

    fn radio(&mut self, text: &str, id: i32, rect: (i32, i32, i32, i32), first: bool, page: usize) {
        let group = if first { WS_GROUP | WS_TABSTOP } else { 0 };
        let hwnd = self.ctl(
            w!("BUTTON"),
            text,
            BS_AUTORADIOBUTTON | group,
            rect,
            id,
            page,
        );
        unsafe {
            let _ = SetWindowSubclass(hwnd, Some(radio_subclass), id as usize, 0);
        }
    }

    fn push_button(
        &mut self,
        text: &str,
        id: i32,
        rect: (i32, i32, i32, i32),
        page: usize,
        is_primary: bool,
    ) -> HWND {
        let hwnd = self.ctl(
            w!("BUTTON"),
            text,
            if is_primary {
                BS_DEFPUSHBUTTON
            } else {
                BS_PUSHBUTTON
            } | WS_TABSTOP,
            rect,
            id,
            page,
        );
        unsafe {
            let _ = SetWindowSubclass(
                hwnd,
                Some(button_subclass),
                id as usize,
                if is_primary { 1 } else { 0 },
            );
        }
        hwnd
    }

    fn show_page(&mut self, page: usize) {
        self.current_page = page;
        for (h, p) in &self.controls {
            // SAFETY: showing/hiding our own controls.
            unsafe {
                let _ = ShowWindow(
                    *h,
                    if *p == page || *p == usize::MAX {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
        }
        for b in &self.sidebar_buttons {
            unsafe {
                let _ = InvalidateRect(Some(*b), None, true);
            }
        }
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, true);
        }
    }
}

// ------------------------------------------------------------------ building

fn build(win: &mut Win, settings: &Settings, page: usize) {
    let hwnd = win.hwnd;

    // Invisible tab control to preserve compatibility with TCM messages
    let tab = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("SysTabControl32"),
            PCWSTR::null(),
            WINDOW_STYLE(WS_CHILD),
            0,
            0,
            0,
            0,
            Some(hwnd),
            Some(HMENU(0x400 as *mut c_void)),
            None,
            None,
        )
        .unwrap_or_default()
    };
    win.tab = tab;

    // Modern Frameless Window Title Buttons
    let btn_w = 46;
    let btn_h = 32;
    let b_min = win.ctl(
        w!("BUTTON"),
        "",
        BS_PUSHBUTTON | WS_TABSTOP,
        (720 - btn_w * 2, 0, btn_w, btn_h),
        ID_TITLE_MIN,
        usize::MAX,
    );
    unsafe {
        let _ = SetWindowSubclass(b_min, Some(title_button_subclass), ID_TITLE_MIN as usize, 0);
    }
    let b_close = win.ctl(
        w!("BUTTON"),
        "",
        BS_PUSHBUTTON | WS_TABSTOP,
        (720 - btn_w, 0, btn_w, btn_h),
        ID_TITLE_CLOSE,
        usize::MAX,
    );
    unsafe {
        let _ = SetWindowSubclass(
            b_close,
            Some(title_button_subclass),
            ID_TITLE_CLOSE as usize,
            0,
        );
    }

    // Sidebar navigation buttons (wider with comfortable padding)
    for (i, name) in PAGES.iter().enumerate() {
        let b = win.push_button(
            name,
            ID_TAB_BASE + i as i32,
            (8, 68 + 36 * i as i32, 170, 32),
            usize::MAX,
            false,
        );
        win.sidebar_buttons.push(b);
    }

    // ---- Page 0: Cài đặt chung
    win.section_title("Kiểu gõ", (212, 42, 480, 20), 0);
    win.radio("Telex", ID_TELEX, (220, 66, 90, 22), true, 0);
    win.radio("VNI", ID_VNI, (320, 66, 90, 22), false, 0);
    win.radio("VIQR", ID_VIQR, (420, 66, 90, 22), false, 0);

    win.section_title("Bảng mã & Đặt dấu", (212, 98, 480, 20), 0);
    win.label("Bảng mã:", (220, 122, 75, 20), 0);
    let enc = win.combobox((298, 116, 280, 90), ID_ENCODING, 0);
    add_string(enc, CB_ADDSTRING, "Unicode dựng sẵn (khuyên dùng)");
    add_string(enc, CB_ADDSTRING, "Unicode tổ hợp");

    win.label("Đặt dấu:", (220, 154, 75, 20), 0);
    win.radio(
        "Kiểu mới: hoà, thuý",
        ID_TONE_NEW,
        (298, 152, 175, 22),
        true,
        0,
    );
    win.radio(
        "Kiểu cũ: hòa, thúy",
        ID_TONE_OLD,
        (485, 152, 175, 22),
        false,
        0,
    );

    win.section_title("Tùy chọn soạn thảo", (212, 190, 480, 20), 0);
    let left = [
        ("Kiểm tra chính tả", ID_SPELL),
        ("Tự khôi phục từ không phải TV", ID_RESTORE),
        ("Bỏ dấu tự do (sau cả từ)", ID_FREE),
        ("Esc trả lại phím đã gõ", ID_ESC),
        ("Backspace xóa theo phím đã gõ", ID_RAWBS),
    ];
    for (i, (t, id)) in left.iter().enumerate() {
        win.check(t, *id, (220, 216 + 25 * i as i32, 235, 22), 0);
    }
    let right = [
        ("Telex: w đầu từ thành ư", ID_W),
        ("Telex: [ ] thành ơ ư", ID_BRACKETS),
        ("Cho phép phụ âm f, j, w, z", ID_FOREIGN),
        ("Bật gõ tắt (macro)", ID_MACROS_ON),
        ("Khởi động cùng Windows", ID_AUTOSTART),
    ];
    for (i, (t, id)) in right.iter().enumerate() {
        win.check(t, *id, (465, 216 + 25 * i as i32, 235, 22), 0);
    }
    win.label(
        "Thay đổi có hiệu lực ngay khi bấm Áp dụng hoặc Đóng.",
        (220, 350, 475, 20),
        0,
    );

    // ---- Page 1: Phím tắt
    win.section_title("Phím tắt chuyển đổi", (212, 42, 480, 20), 1);
    win.check("Ctrl", ID_CTRL, (220, 76, 70, 22), 1);
    win.check("Shift", ID_SHIFT, (298, 76, 70, 22), 1);
    win.check("Alt", ID_ALT, (376, 76, 70, 22), 1);
    win.check("Win", ID_WIN, (454, 76, 70, 22), 1);
    win.label("Phím thêm:", (220, 116, 90, 20), 1);
    let key = win.combobox((315, 112, 180, 220), ID_KEY, 1);
    add_string(key, CB_ADDSTRING, "(không có)");
    for (name, _) in &win.keys.clone() {
        add_string(key, CB_ADDSTRING, name);
    }
    win.label(
        "Ví dụ: Ctrl + Shift (mặc định), hoặc Alt + Z. Bỏ chọn hết các phím Ctrl/Shift/Alt/Win\nđể tắt phím tắt (vẫn bấm vào biểu tượng khay để đổi).",
        (220, 154, 475, 44),
        1,
    );

    // ---- Page 2: Gõ tắt
    win.section_title("Quản lý từ gõ tắt", (212, 42, 480, 20), 2);
    win.label(
        "Gõ từ tắt rồi nhấn Space, Enter hoặc Tab để mở rộng.",
        (212, 66, 480, 20),
        2,
    );
    win.ctl(
        w!("LISTBOX"),
        "",
        LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | WS_BORDER | WS_VSCROLL | WS_TABSTOP,
        (212, 92, 245, 260),
        ID_MACRO_LIST,
        2,
    );
    win.label("Từ tắt:", (472, 92, 225, 18), 2);
    win.ctl(
        w!("EDIT"),
        "",
        WS_BORDER | ES_AUTOHSCROLL | WS_TABSTOP,
        (472, 112, 225, 24),
        ID_MACRO_TRIGGER,
        2,
    );
    win.label("Mở rộng thành:", (472, 144, 225, 18), 2);
    win.ctl(
        w!("EDIT"),
        "",
        WS_BORDER | ES_AUTOHSCROLL | WS_TABSTOP,
        (472, 164, 225, 24),
        ID_MACRO_TEXT,
        2,
    );
    win.push_button("Thêm / Lưu", ID_MACRO_ADD, (472, 200, 120, 28), 2, false);
    win.push_button("Xóa", ID_MACRO_DEL, (604, 200, 92, 28), 2, false);

    // ---- Page 3: Ứng dụng loại trừ
    win.section_title("Ứng dụng loại trừ", (212, 42, 480, 20), 3);
    win.label(
        "pKey không can thiệp vào các chương trình này (game, phần mềm có bộ gõ riêng...).",
        (212, 66, 480, 20),
        3,
    );
    win.ctl(
        w!("LISTBOX"),
        "",
        LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | WS_BORDER | WS_VSCROLL | WS_TABSTOP,
        (212, 92, 245, 260),
        ID_EXCL_LIST,
        3,
    );
    win.label("Tên chương trình (ví dụ game.exe):", (472, 92, 225, 18), 3);
    win.ctl(
        w!("EDIT"),
        "",
        WS_BORDER | ES_AUTOHSCROLL | WS_TABSTOP,
        (472, 112, 225, 24),
        ID_EXCL_EDIT,
        3,
    );
    win.push_button("Thêm", ID_EXCL_ADD, (472, 146, 105, 28), 3, false);
    win.push_button("Xóa", ID_EXCL_DEL, (592, 146, 105, 28), 3, false);

    // ---- Page 4: Thông tin
    win.section_title("Thông tin", (212, 42, 480, 20), 4);
    win.label("pKey - Bộ gõ Tiếng Việt", (212, 76, 480, 22), 4);
    win.label("Phiên bản: 0.1.0", (212, 106, 480, 20), 4);
    win.label(
        "Bộ gõ tiếng Việt tốc độ cao cho Windows.",
        (212, 136, 480, 20),
        4,
    );
    win.label(
        "Phím tắt chuyển đổi mặc định: Ctrl + Shift",
        (212, 166, 480, 20),
        4,
    );
    win.label(
        "Mã nguồn mở theo giấy phép MIT. Bản quyền © 2026.",
        (212, 196, 480, 20),
        4,
    );

    // ---- Bottom Buttons (always visible)
    let y = 394;
    win.push_button("Đóng", ID_OK, (420, y, 84, 32), usize::MAX, true);
    win.push_button("Hủy", ID_CANCEL, (514, y, 84, 32), usize::MAX, false);
    win.push_button("Áp dụng", ID_APPLY, (608, y, 96, 32), usize::MAX, false);

    fill(win, settings);
    win.show_page(page.min(PAGES.len() - 1));
}

fn macro_line(m: &Macro) -> String {
    let flat = m.expansion.replace(['\r', '\n'], " ");
    let short: String = flat.chars().take(40).collect();
    format!("{}  →  {}", m.trigger, short)
}

fn refresh_macro_list(win: &Win) {
    let list = item(win.hwnd, ID_MACRO_LIST);
    send(list, LB_RESETCONTENT, 0, 0);
    for m in &win.macros {
        add_string(list, LB_ADDSTRING, &macro_line(m));
    }
}

fn refresh_excluded_list(win: &Win) {
    let list = item(win.hwnd, ID_EXCL_LIST);
    send(list, LB_RESETCONTENT, 0, 0);
    for e in &win.excluded {
        add_string(list, LB_ADDSTRING, e);
    }
}

/// Put `s` into the controls.
fn fill(win: &Win, s: &Settings) {
    let h = win.hwnd;
    set_checked(h, ID_TELEX, s.scheme == Scheme::Telex);
    set_checked(h, ID_VNI, s.scheme == Scheme::Vni);
    set_checked(h, ID_VIQR, s.scheme == Scheme::Viqr);
    send(
        item(h, ID_ENCODING),
        CB_SETCURSEL,
        usize::from(s.nfd_output),
        0,
    );
    set_checked(h, ID_TONE_NEW, s.tone_style == ToneStyle::Modern);
    set_checked(h, ID_TONE_OLD, s.tone_style == ToneStyle::Traditional);
    set_checked(h, ID_SPELL, s.spell_check);
    set_checked(h, ID_RESTORE, s.auto_restore);
    set_checked(h, ID_FREE, s.free_marking);
    set_checked(h, ID_ESC, s.esc_restores_raw);
    set_checked(h, ID_RAWBS, s.raw_backspace);
    set_checked(h, ID_W, s.telex_w_standalone);
    set_checked(h, ID_BRACKETS, s.telex_brackets);
    set_checked(h, ID_FOREIGN, s.foreign_onsets);
    set_checked(h, ID_MACROS_ON, s.macros_enabled);
    set_checked(h, ID_AUTOSTART, s.autostart);
    set_checked(h, ID_CTRL, s.toggle.ctrl);
    set_checked(h, ID_SHIFT, s.toggle.shift);
    set_checked(h, ID_ALT, s.toggle.alt);
    set_checked(h, ID_WIN, s.toggle.meta);
    let index = s
        .toggle
        .key
        .and_then(|k| win.keys.iter().position(|(_, c)| *c == k))
        .map_or(0, |i| i + 1);
    send(item(h, ID_KEY), CB_SETCURSEL, index, 0);
    refresh_macro_list(win);
    refresh_excluded_list(win);
}

/// Read the controls back.
fn collect(win: &Win) -> Settings {
    let h = win.hwnd;
    let sel = send(item(h, ID_KEY), CB_GETCURSEL, 0, 0);
    let key = (sel > 0)
        .then(|| win.keys.get(sel as usize - 1).map(|(_, c)| *c))
        .flatten();
    Settings {
        scheme: if checked(h, ID_VNI) {
            Scheme::Vni
        } else if checked(h, ID_VIQR) {
            Scheme::Viqr
        } else {
            Scheme::Telex
        },
        nfd_output: send(item(h, ID_ENCODING), CB_GETCURSEL, 0, 0) == 1,
        tone_style: if checked(h, ID_TONE_OLD) {
            ToneStyle::Traditional
        } else {
            ToneStyle::Modern
        },
        spell_check: checked(h, ID_SPELL),
        auto_restore: checked(h, ID_RESTORE),
        free_marking: checked(h, ID_FREE),
        esc_restores_raw: checked(h, ID_ESC),
        raw_backspace: checked(h, ID_RAWBS),
        telex_w_standalone: checked(h, ID_W),
        telex_brackets: checked(h, ID_BRACKETS),
        foreign_onsets: checked(h, ID_FOREIGN),
        macros_enabled: checked(h, ID_MACROS_ON),
        autostart: checked(h, ID_AUTOSTART),
        toggle: HotkeyChoice {
            ctrl: checked(h, ID_CTRL),
            shift: checked(h, ID_SHIFT),
            alt: checked(h, ID_ALT),
            meta: checked(h, ID_WIN),
            key,
        },
        excluded: win.excluded.clone(),
    }
}

fn warn(hwnd: HWND, text: &str) {
    // SAFETY: owned strings.
    unsafe {
        MessageBoxW(
            Some(hwnd),
            &HSTRING::from(text),
            w!("pKey"),
            MB_OK | MB_ICONWARNING,
        );
    }
}

/// Write the settings and macros into the running app. Returns whether it worked.
fn apply(win: &Win) -> bool {
    let settings = collect(win);
    let macros = win.macros.clone();
    let result = with_app(|app| {
        let mut probe = app.ctl.runtime().config().clone();
        settings.apply_to(&mut probe)?;
        app.change(|c| {
            let _ = settings.apply_to(c);
        });
        if settings.autostart != sys::autostart_enabled() && !sys::set_autostart(settings.autostart)
        {
            return Err("Không đổi được chế độ khởi động cùng Windows");
        }
        if macros != app.ctl.runtime().macros() {
            app.ctl
                .runtime_mut()
                .update_macros(macros)
                .map_err(|_| "Danh sách gõ tắt không hợp lệ (trùng từ tắt hoặc để trống)")?;
        }
        Ok(())
    });
    match result {
        Some(Ok(())) => true,
        Some(Err(msg)) => {
            warn(win.hwnd, msg);
            false
        }
        None => false,
    }
}

// ------------------------------------------------------------------ events

fn on_macro_selected(win: &Win) {
    let i = send(item(win.hwnd, ID_MACRO_LIST), LB_GETCURSEL, 0, 0);
    if let Some(m) = usize::try_from(i).ok().and_then(|i| win.macros.get(i)) {
        set_text(item(win.hwnd, ID_MACRO_TRIGGER), &m.trigger);
        set_text(item(win.hwnd, ID_MACRO_TEXT), &m.expansion);
    }
}

fn on_macro_add(win: &mut Win) {
    let trigger = text_of(item(win.hwnd, ID_MACRO_TRIGGER)).trim().to_string();
    let expansion = text_of(item(win.hwnd, ID_MACRO_TEXT));
    if trigger.is_empty() || expansion.is_empty() {
        warn(win.hwnd, "Nhập cả từ tắt và nội dung mở rộng.");
        return;
    }
    if trigger.chars().any(|c| c.is_whitespace()) {
        warn(win.hwnd, "Từ tắt không được có dấu cách.");
        return;
    }
    match win
        .macros
        .iter_mut()
        .find(|m| m.trigger.to_lowercase() == trigger.to_lowercase())
    {
        Some(m) => m.expansion = expansion,
        None => {
            let mut m = Macro::new(&trigger, &expansion);
            m.id = ime_runtime::ime_macro::store::new_id();
            win.macros.push(m);
        }
    }
    refresh_macro_list(win);
    set_text(item(win.hwnd, ID_MACRO_TRIGGER), "");
    set_text(item(win.hwnd, ID_MACRO_TEXT), "");
}

fn on_macro_delete(win: &mut Win) {
    let i = send(item(win.hwnd, ID_MACRO_LIST), LB_GETCURSEL, 0, 0);
    if let Ok(i) = usize::try_from(i) {
        if i < win.macros.len() {
            win.macros.remove(i);
            refresh_macro_list(win);
        }
    }
}

fn on_excluded_add(win: &mut Win) {
    let raw = text_of(item(win.hwnd, ID_EXCL_EDIT));
    let Some(exe) = normalize_exe(&raw) else {
        warn(win.hwnd, "Nhập tên chương trình, ví dụ: game.exe");
        return;
    };
    if !win.excluded.iter().any(|e| e.eq_ignore_ascii_case(&exe)) {
        win.excluded.push(exe);
    }
    refresh_excluded_list(win);
    set_text(item(win.hwnd, ID_EXCL_EDIT), "");
}

fn on_excluded_delete(win: &mut Win) {
    let i = send(item(win.hwnd, ID_EXCL_LIST), LB_GETCURSEL, 0, 0);
    if let Ok(i) = usize::try_from(i) {
        if i < win.excluded.len() {
            win.excluded.remove(i);
            refresh_excluded_list(win);
        }
    }
}

fn close() {
    let hwnd = WIN.with(|w| w.borrow().as_ref().map(|w| w.hwnd));
    if let Some(h) = hwnd {
        // SAFETY: destroying our own window.
        unsafe {
            let _ = DestroyWindow(h);
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let note = ((wparam.0 >> 16) & 0xFFFF) as u32;
            if (note == CBN_SELCHANGE as u32 || note == 7 || note == 8) && lparam.0 != 0 {
                unsafe {
                    let _ = InvalidateRect(Some(HWND(lparam.0 as *mut c_void)), None, false);
                }
            }
            let mut done = false;
            WIN.with(|w| {
                let Ok(mut g) = w.try_borrow_mut() else {
                    return;
                };
                let Some(win) = g.as_mut() else { return };
                match (id, note) {
                    (ID_OK, _) => done = apply(win),
                    (ID_APPLY, _) => {
                        apply(win);
                    }
                    (ID_CANCEL, _) => done = true,
                    (ID_TITLE_MIN, _) => unsafe {
                        let _ = ShowWindow(win.hwnd, SW_MINIMIZE);
                    },
                    (ID_TITLE_CLOSE, _) => done = true,
                    (400..=404, _) => {
                        let page = (id - 400) as usize;
                        win.show_page(page);
                    }
                    (ID_MACRO_LIST, LBN_SELCHANGE) => on_macro_selected(win),
                    (ID_MACRO_ADD, BN_CLICKED) => on_macro_add(win),
                    (ID_MACRO_DEL, BN_CLICKED) => on_macro_delete(win),
                    (ID_EXCL_ADD, BN_CLICKED) => on_excluded_add(win),
                    (ID_EXCL_DEL, BN_CLICKED) => on_excluded_delete(win),
                    _ => {}
                }
            });
            if done {
                close();
            }
            LRESULT(0)
        }
        WM_MEASUREITEM => {
            let mis = unsafe { &mut *(lparam.0 as *mut MEASUREITEMSTRUCT) };
            if mis.CtlType == ODT_COMBOBOX {
                WIN.with(|w| {
                    if let Ok(g) = w.try_borrow() {
                        if let Some(win) = g.as_ref() {
                            mis.itemHeight = win.s(28) as u32;
                        }
                    }
                });
                return LRESULT(1);
            }
            LRESULT(0)
        }
        WM_DRAWITEM => {
            let dis = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
            if dis.CtlType == ODT_COMBOBOX {
                if dis.itemID == u32::MAX {
                    return LRESULT(1);
                }
                let is_selected = (dis.itemState.0 & 0x0001) != 0; // ODS_SELECTED
                let is_edit = (dis.itemState.0 & 0x1000) != 0; // ODS_COMBOBOXEDIT
                if !is_edit {
                    let hdc = dis.hDC;
                    let rc = dis.rcItem;
                    unsafe {
                        let bg_col = if is_selected {
                            0x00FF_840D // Electric Blue #0D84FF
                        } else {
                            0x00FF_FFFF // Pure white #FFFFFF
                        };
                        let brush = CreateSolidBrush(COLORREF(bg_col));
                        FillRect(hdc, &rc, brush);
                        let _ = DeleteObject(brush.into());

                        let text_col = if is_selected {
                            0x00FF_FFFF // Crisp White
                        } else {
                            0x002A_231E // Charcoal #1E232A
                        };
                        SetBkMode(hdc, TRANSPARENT);
                        SetTextColor(hdc, COLORREF(text_col));

                        WIN.with(|w| {
                            if let Ok(g) = w.try_borrow() {
                                if let Some(win) = g.as_ref() {
                                    let old_font = SelectObject(hdc, win.font.into());
                                    let mut text = [0u16; 256];
                                    let len = SendMessageW(
                                        dis.hwndItem,
                                        CB_GETLBTEXT,
                                        Some(WPARAM(dis.itemID as usize)),
                                        Some(LPARAM(text.as_mut_ptr() as isize)),
                                    )
                                    .0;
                                    if len > 0 {
                                        let mut text_rc = RECT {
                                            left: rc.left + win.s(12),
                                            top: rc.top,
                                            right: rc.right - win.s(8),
                                            bottom: rc.bottom,
                                        };
                                        DrawTextW(
                                            hdc,
                                            &mut text[..len as usize],
                                            &mut text_rc,
                                            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                                        );
                                    }
                                    SelectObject(hdc, old_font);
                                }
                            }
                        });
                    }
                }
                return LRESULT(1);
            }
            LRESULT(0)
        }
        WM_NOTIFY => {
            // SAFETY: WM_NOTIFY always carries a valid NMHDR.
            let hdr = unsafe { &*(lparam.0 as *const NMHDR) };
            if hdr.code == TCN_SELCHANGE {
                WIN.with(|w| {
                    if let Ok(mut g) = w.try_borrow_mut() {
                        if let Some(win) = g.as_mut() {
                            let page = send(win.tab, TCM_GETCURSEL, 0, 0).max(0) as usize;
                            win.show_page(page);
                        }
                    }
                });
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            WIN.with(|w| {
                if let Ok(g) = w.try_borrow() {
                    if let Some(win) = g.as_ref() {
                        unsafe {
                            let mut client = RECT::default();
                            let _ = GetClientRect(hwnd, &mut client);
                            let sw = win.s(186);

                            // 1. Sidebar background
                            let side_rect = RECT {
                                left: 0,
                                top: 0,
                                right: sw,
                                bottom: client.bottom,
                            };
                            FillRect(hdc, &side_rect, win.sidebar_brush);

                            // 2. Main content background
                            let main_rect = RECT {
                                left: sw,
                                top: 0,
                                right: client.right,
                                bottom: client.bottom,
                            };
                            FillRect(hdc, &main_rect, win.bg_brush);

                            // 3. Vertical separator
                            let line_rect = RECT {
                                left: sw - 1,
                                top: 0,
                                right: sw,
                                bottom: client.bottom,
                            };
                            FillRect(hdc, &line_rect, win.border_brush);

                            // 4. Bottom horizontal separator above buttons
                            let bot_line = RECT {
                                left: sw,
                                top: win.s(384),
                                right: client.right,
                                bottom: win.s(385),
                            };
                            FillRect(hdc, &bot_line, win.border_brush);

                            // 5. Draw rounded container cards for active page sections
                            let old_pen = SelectObject(hdc, win.card_pen.into());
                            let old_brush = SelectObject(hdc, win.card_brush.into());

                            let card_l = win.s(198);
                            let card_r = win.s(708);
                            match win.current_page {
                                0 => {
                                    // Card 1: Input method & encoding
                                    let _ = RoundRect(
                                        hdc,
                                        card_l,
                                        win.s(36),
                                        card_r,
                                        win.s(178),
                                        win.s(8),
                                        win.s(8),
                                    );
                                    // Card 2: Typing options
                                    let _ = RoundRect(
                                        hdc,
                                        card_l,
                                        win.s(184),
                                        card_r,
                                        win.s(376),
                                        win.s(8),
                                        win.s(8),
                                    );
                                }
                                1 => {
                                    // Card: Hotkey
                                    let _ = RoundRect(
                                        hdc,
                                        card_l,
                                        win.s(36),
                                        card_r,
                                        win.s(214),
                                        win.s(8),
                                        win.s(8),
                                    );
                                }
                                2 | 3 => {
                                    // Card: Macro / Excluded
                                    let _ = RoundRect(
                                        hdc,
                                        card_l,
                                        win.s(36),
                                        card_r,
                                        win.s(376),
                                        win.s(8),
                                        win.s(8),
                                    );
                                }
                                4 => {
                                    // Card: About
                                    let _ = RoundRect(
                                        hdc,
                                        card_l,
                                        win.s(36),
                                        card_r,
                                        win.s(238),
                                        win.s(8),
                                        win.s(8),
                                    );
                                }
                                _ => {}
                            }
                            SelectObject(hdc, old_pen);
                            SelectObject(hdc, old_brush);

                            // 6. Sidebar Brand Title & App Icon
                            if let Some(icon) = win.app_icon {
                                let icon_size = win.s(32);
                                let icon_x = win.s(14);
                                let icon_y = win.s(16);
                                let _ = DrawIconEx(
                                    hdc, icon_x, icon_y, icon, icon_size, icon_size, 0, None,
                                    DI_NORMAL,
                                );
                            }

                            let text_left = if win.app_icon.is_some() {
                                win.s(52)
                            } else {
                                win.s(18)
                            };

                            let old_font = SelectObject(hdc, win.title_font.into());
                            SetBkMode(hdc, TRANSPARENT);
                            SetTextColor(hdc, COLORREF(0x002A_170F));
                            let mut brand: Vec<u16> = "pKey".encode_utf16().collect();
                            let mut b_rect = RECT {
                                left: text_left,
                                top: win.s(14),
                                right: sw,
                                bottom: win.s(34),
                            };
                            DrawTextW(
                                hdc,
                                &mut brand,
                                &mut b_rect,
                                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                            );

                            // Subtitle
                            SelectObject(hdc, win.font.into());
                            SetTextColor(hdc, COLORREF(0x0083_7063));
                            let mut sub: Vec<u16> = "Bộ gõ tiếng Việt".encode_utf16().collect();
                            let mut s_rect = RECT {
                                left: text_left,
                                top: win.s(34),
                                right: sw,
                                bottom: win.s(52),
                            };
                            DrawTextW(
                                hdc,
                                &mut sub,
                                &mut s_rect,
                                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                            );

                            SelectObject(hdc, old_font);
                        }
                    }
                }
            });
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_CTLCOLORDLG | WM_CTLCOLORBTN => WIN.with(|w| {
            if let Ok(g) = w.try_borrow() {
                if let Some(win) = g.as_ref() {
                    return LRESULT(win.bg_brush.0 as isize);
                }
            }
            LRESULT(0)
        }),
        WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as *mut c_void);
            WIN.with(|w| {
                if let Ok(g) = w.try_borrow() {
                    if let Some(win) = g.as_ref() {
                        unsafe {
                            SetBkMode(hdc, TRANSPARENT);
                            SetTextColor(hdc, COLORREF(0x002A_231E));
                        }
                        return LRESULT(win.card_brush.0 as isize);
                    }
                }
                LRESULT(0)
            })
        }
        WM_CTLCOLORLISTBOX => {
            let hdc = HDC(wparam.0 as *mut c_void);
            let lb = HWND(lparam.0 as *mut c_void);
            unsafe {
                let _ = SetWindowTheme(lb, w!("CFD"), None);
            }
            WIN.with(|w| {
                if let Ok(g) = w.try_borrow() {
                    if let Some(win) = g.as_ref() {
                        unsafe {
                            SetBkColor(hdc, COLORREF(0x00FF_FFFF));
                            SetTextColor(hdc, COLORREF(0x002A_231E));
                        }
                        return LRESULT(win.card_brush.0 as isize);
                    }
                }
                LRESULT(0)
            })
        }
        WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as *mut c_void);
            WIN.with(|w| {
                if let Ok(g) = w.try_borrow() {
                    if let Some(win) = g.as_ref() {
                        unsafe {
                            SetBkColor(hdc, COLORREF(0x00FF_FFFF));
                            SetTextColor(hdc, COLORREF(0x002A_231E));
                        }
                        return LRESULT(win.card_brush.0 as isize);
                    }
                }
                LRESULT(0)
            })
        }
        WM_NCCALCSIZE => {
            if wparam.0 != 0 {
                return LRESULT(0);
            }
            // SAFETY: default handling.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_NCHITTEST => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut pt = POINT { x, y };
            unsafe {
                let _ = ScreenToClient(hwnd, &mut pt);
            }
            WIN.with(|w| {
                if let Ok(g) = w.try_borrow() {
                    if let Some(win) = g.as_ref() {
                        // Title control buttons in top-right: pass to buttons (HTCLIENT = 1)
                        if pt.x >= win.s(720 - 92) && pt.y < win.s(32) {
                            return LRESULT(1);
                        }
                        // Top bar or sidebar header allows dragging (HTCAPTION = 2)
                        if pt.y < win.s(36) || (pt.x < win.s(186) && pt.y < win.s(56)) {
                            return LRESULT(2);
                        }
                    }
                }
                LRESULT(1)
            })
        }
        WM_CLOSE => {
            close();
            LRESULT(0)
        }
        WM_DESTROY => {
            WIN.with(|w| {
                if let Some(win) = w.borrow_mut().take() {
                    // SAFETY: GDI resources were created by us.
                    unsafe {
                        if let Some(icon) = win.app_icon {
                            let _ = DestroyIcon(icon);
                        }
                        let _ = DeleteObject(win.font.into());
                        let _ = DeleteObject(win.bold_font.into());
                        let _ = DeleteObject(win.title_font.into());
                        let _ = DeleteObject(win.bg_brush.into());
                        let _ = DeleteObject(win.sidebar_brush.into());
                        let _ = DeleteObject(win.card_brush.into());
                        let _ = DeleteObject(win.border_brush.into());
                        let _ = DeleteObject(win.card_pen.into());
                    }
                }
            });
            LRESULT(0)
        }
        // SAFETY: default handling of a message we do not process.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Give a message to the Settings window's keyboard navigation (Tab, Enter, Esc). Returns true if
/// it was consumed.
pub fn is_dialog_message(msg: &MSG) -> bool {
    let hwnd = WIN.with(|w| w.try_borrow().ok().and_then(|g| g.as_ref().map(|w| w.hwnd)));
    // SAFETY: valid window and message.
    hwnd.is_some_and(|h| unsafe { IsDialogMessageW(h, msg) }.as_bool())
}

fn load_app_icon(instance: windows::Win32::Foundation::HINSTANCE, size: i32) -> Option<HICON> {
    unsafe {
        // Try embedded icon resource #1 from app.rc first
        let handle = LoadImageW(
            Some(instance),
            PCWSTR(1 as *const u16),
            IMAGE_ICON,
            size,
            size,
            LR_DEFAULTCOLOR,
        );
        if let Ok(h) = handle {
            if !h.is_invalid() {
                return Some(HICON(h.0));
            }
        }
        // Try loading from app.ico on disk (e.g. during cargo test where test runner lacks resources)
        for path in [w!("app.ico"), w!("platform\\windows-app\\app.ico")] {
            if let Ok(h) = LoadImageW(
                None,
                path,
                IMAGE_ICON,
                size,
                size,
                windows::Win32::UI::WindowsAndMessaging::IMAGE_FLAGS(0x0010 | LR_DEFAULTCOLOR.0),
            ) {
                if !h.is_invalid() {
                    return Some(HICON(h.0));
                }
            }
        }
        // Fallback to dynamic renderer
        crate::win::tray::make_icon("p", (0, 150, 100))
    }
}

/// Open the window (or bring it to the front).
pub fn open(settings: Settings, macros: Vec<Macro>, page: usize) {
    let existing = WIN.with(|w| w.try_borrow().ok().and_then(|g| g.as_ref().map(|w| w.hwnd)));
    if let Some(h) = existing {
        // SAFETY: our window.
        unsafe {
            let _ = SetForegroundWindow(h);
        }
        return;
    }
    // SAFETY: standard window creation; everything created here is released in WM_DESTROY.
    unsafe {
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TAB_CLASSES,
        });
        let screen = GetDC(None);
        let dpi = GetDeviceCaps(Some(screen), LOGPIXELSX).max(96);
        ReleaseDC(None, screen);
        let font = CreateFontW(
            -(10 * dpi / 72),
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            CLEARTYPE_QUALITY,
            0,
            &HSTRING::from("Segoe UI"),
        );
        let bold_font = CreateFontW(
            -(10 * dpi / 72),
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            CLEARTYPE_QUALITY,
            0,
            &HSTRING::from("Segoe UI"),
        );
        let title_font = CreateFontW(
            -(13 * dpi / 72),
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            CLEARTYPE_QUALITY,
            0,
            &HSTRING::from("Segoe UI"),
        );
        let bg_brush = CreateSolidBrush(COLORREF(0x00F8_F4F2));
        let sidebar_brush = CreateSolidBrush(COLORREF(0x00F4_EEEA));
        let card_brush = CreateSolidBrush(COLORREF(0x00FF_FFFF));
        let border_brush = CreateSolidBrush(COLORREF(0x00E4_DDD6));
        let card_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00E4_DDD6));

        let instance =
            windows::Win32::Foundation::HINSTANCE(GetModuleHandleW(None).unwrap_or_default().0);
        let class = w!("pKey.Settings");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance,
            lpszClassName: class,
            hbrBackground: bg_brush,
            ..Default::default()
        });
        let (w_px, h_px) = (720 * dpi / 96, 444 * dpi / 96);
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_CONTROLPARENT),
            class,
            w!("pKey - Cài đặt"),
            WINDOW_STYLE(WS_FRAMELESS_APP),
            (GetSystemMetrics(SM_CXSCREEN) - w_px) / 2,
            (GetSystemMetrics(SM_CYSCREEN) - h_px) / 2,
            w_px,
            h_px,
            None,
            None,
            Some(instance),
            None,
        ) else {
            return;
        };

        // Enable native hardware-accelerated DWM drop shadow for frameless window
        let margins = MARGINS {
            cxLeftWidth: 0,
            cxRightWidth: 0,
            cyTopHeight: 1,
            cyBottomHeight: 0,
        };
        let _ = windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea(hwnd, &margins);

        let icon_sm = load_app_icon(instance, GetSystemMetrics(SM_CXSMICON));
        let icon_lg = load_app_icon(instance, GetSystemMetrics(SM_CXICON));
        if let Some(icon) = icon_sm {
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(0)),
                Some(LPARAM(icon.0 as isize)),
            );
        }
        if let Some(icon) = icon_lg {
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(1)),
                Some(LPARAM(icon.0 as isize)),
            );
        }

        // Set Light Mode for title bar and rounded corners
        let light_mode = windows::core::BOOL(0);
        let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_USE_IMMERSIVE_DARK_MODE,
            &light_mode as *const _ as *const c_void,
            std::mem::size_of::<windows::core::BOOL>() as u32,
        );
        let corners = windows::Win32::Graphics::Dwm::DWMWCP_ROUND;
        let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_WINDOW_CORNER_PREFERENCE,
            &corners as *const _ as *const c_void,
            std::mem::size_of::<windows::Win32::Graphics::Dwm::DWM_WINDOW_CORNER_PREFERENCE>()
                as u32,
        );
        let app_icon = load_app_icon(instance, 32 * dpi / 96);
        let mut win = Win {
            hwnd,
            tab: HWND::default(),
            font,
            bold_font,
            title_font,
            dpi,
            current_page: page.min(PAGES.len() - 1),
            bg_brush,
            sidebar_brush,
            card_brush,
            border_brush,
            card_pen,
            app_icon,
            controls: Vec::new(),
            sidebar_buttons: Vec::new(),
            macros,
            excluded: settings.excluded.clone(),
            keys: key_choices(),
        };
        build(&mut win, &settings, page.min(PAGES.len() - 1));
        WIN.with(|w| *w.borrow_mut() = Some(win));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}
