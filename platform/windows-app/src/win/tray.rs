//! Tray icon (a "V" or "E" tile drawn at run time) and its menu.

use ime_runtime::ime_config::Config;
use ime_runtime::ime_core::{InputMode, Scheme, ToneStyle};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject,
    DrawTextW, GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY, DIB_RGB_COLORS, DT_CENTER, DT_SINGLELINE,
    DT_VCENTER, FW_BOLD, HBITMAP, TRANSPARENT,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW,
    SetForegroundWindow, TrackPopupMenu, HICON, ICONINFO, MF_CHECKED, MF_POPUP, MF_SEPARATOR,
    MF_STRING, MF_UNCHECKED, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_NULL,
};

pub const WM_TRAY: u32 = 0x8000 + 1; // WM_APP + 1
pub const TRAY_ID: u32 = 1;

pub const IDM_TOGGLE: u32 = 1;
pub const IDM_SETTINGS: u32 = 2;
pub const IDM_TELEX: u32 = 10;
pub const IDM_VNI: u32 = 11;
pub const IDM_VIQR: u32 = 12;
pub const IDM_MODERN: u32 = 20;
pub const IDM_TRADITIONAL: u32 = 21;
pub const IDM_SPELL: u32 = 30;
pub const IDM_RESTORE: u32 = 31;
pub const IDM_FREE_MARKING: u32 = 32;
pub const IDM_MACROS: u32 = 33;
pub const IDM_PAUSE: u32 = 40;
pub const IDM_AUTOSTART: u32 = 41;
pub const IDM_OPEN_DIR: u32 = 50;
pub const IDM_OPEN_CONFIG: u32 = 51;
pub const IDM_EXIT: u32 = 99;

/// A modern rounded squircle badge with crisp bold letter and subtle gradient.
/// Crimson red for Vietnamese, sapphire blue for English.
pub fn make_icon(letter: &str, rgb: (u8, u8, u8)) -> Option<HICON> {
    const N: i32 = 32;
    // SAFETY: GDI calls with handles created and released in this function.
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: N,
                biHeight: -N, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color: HBITMAP =
            CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let old = SelectObject(dc, color.into());
        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u32>(), (N * N) as usize);

        // Prepare 1bpp mask: 1 = transparent, 0 = opaque.
        // N = 32 -> 4 bytes per scanline, 128 bytes total.
        let mut mask_bytes = [0xFFu8; 128];

        let r_corner = 7.0f32;
        let x_min = 1.0f32;
        let x_max = 30.0f32;
        let y_min = 1.0f32;
        let y_max = 30.0f32;

        let cx_left = x_min + r_corner;
        let cx_right = x_max - r_corner;
        let cy_top = y_min + r_corner;
        let cy_bottom = y_max - r_corner;

        for y in 0..N {
            let y_f = y as f32 + 0.5;
            for x in 0..N {
                let x_f = x as f32 + 0.5;
                let mut inside = x_f >= x_min && x_f <= x_max && y_f >= y_min && y_f <= y_max;
                if inside {
                    // Check corner radiuses
                    let dx = if x_f < cx_left {
                        cx_left - x_f
                    } else if x_f > cx_right {
                        x_f - cx_right
                    } else {
                        0.0
                    };
                    let dy = if y_f < cy_top {
                        cy_top - y_f
                    } else if y_f > cy_bottom {
                        y_f - cy_bottom
                    } else {
                        0.0
                    };
                    if dx > 0.0 && dy > 0.0 && (dx * dx + dy * dy) > (r_corner * r_corner) {
                        inside = false;
                    }
                }

                let idx = (y * N + x) as usize;
                let mask_idx = (y as usize) * 4 + (x as usize / 8);
                let mask_bit = 7 - (x % 8);

                if inside {
                    // Mark opaque in 1bpp mask (bit 0 = opaque)
                    mask_bytes[mask_idx] &= !(1 << mask_bit);

                    // Gradient: slightly brighter at top, richer at bottom
                    let t = (y_f - y_min) / (y_max - y_min);
                    let mut factor = 1.15 - 0.25 * t;

                    // Subtle top specular sheen (Fluent light reflection)
                    if y_f <= y_min + 2.0 {
                        factor += 0.12;
                    }

                    let mut r = ((rgb.0 as f32 * factor).min(255.0) as u32).max(10);
                    let mut g = ((rgb.1 as f32 * factor).min(255.0) as u32).max(10);
                    let mut b = ((rgb.2 as f32 * factor).min(255.0) as u32).max(10);

                    // Concept A: Micro luminous status indicator dot at top-right
                    let dot_cx = 24.0f32;
                    let dot_cy = 7.0f32;
                    let dot_r = 1.75f32;
                    let d2 = (x_f - dot_cx) * (x_f - dot_cx) + (y_f - dot_cy) * (y_f - dot_cy);
                    if d2 <= dot_r * dot_r {
                        // VI mode: warm glowing coral dot; EN mode: electric cyan dot
                        if rgb.0 > rgb.2 {
                            r = 255;
                            g = 195;
                            b = 135;
                        } else {
                            r = 140;
                            g = 235;
                            b = 255;
                        }
                    } else if d2 <= (dot_r + 0.9) * (dot_r + 0.9) {
                        // Soft halo around the dot
                        if rgb.0 > rgb.2 {
                            r = (r + 40).min(255);
                            g = (g + 30).min(255);
                            b = (b + 20).min(255);
                        } else {
                            r = (r + 20).min(255);
                            g = (g + 35).min(255);
                            b = (b + 45).min(255);
                        }
                    }

                    pixels[idx] = 0xFF00_0000 | (r << 16) | (g << 8) | b;
                } else {
                    // Outside squircle: transparent
                    mask_bytes[mask_idx] |= 1 << mask_bit;
                    pixels[idx] = 0x0000_0000;
                }
            }
        }

        // Font scaling: single letter ("V", "E") vs two letters ("VI", "EN")
        let font_height = if letter.len() > 1 { -18 } else { -25 };
        let font = CreateFontW(
            font_height,
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
        let old_font = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, windows::Win32::Foundation::COLORREF(0x00FF_FFFF));
        let mut text: Vec<u16> = letter.encode_utf16().collect();
        let mut rect = RECT {
            left: 0,
            top: if letter.len() > 1 { 2 } else { 1 },
            right: N,
            bottom: N,
        };
        DrawTextW(
            dc,
            &mut text,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );

        // Keep alpha for inside pixels, transparent for outside
        for y in 0..N {
            for x in 0..N {
                let idx = (y * N + x) as usize;
                let mask_idx = (y as usize) * 4 + (x as usize / 8);
                let mask_bit = 7 - (x % 8);
                let is_transparent = (mask_bytes[mask_idx] & (1 << mask_bit)) != 0;
                if is_transparent {
                    pixels[idx] = 0x0000_0000;
                } else {
                    pixels[idx] |= 0xFF00_0000;
                }
            }
        }

        SelectObject(dc, old_font);
        let _ = DeleteObject(font.into());

        let mask = CreateBitmap(N, N, 1, 1, Some(mask_bytes.as_ptr().cast()));
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        })
        .ok();
        SelectObject(dc, old);
        let _ = DeleteObject(mask.into());
        let _ = DeleteObject(color.into());
        let _ = DeleteDC(dc);
        ReleaseDC(None, screen);
        icon
    }
}

fn tip_text(s: &str) -> [u16; 128] {
    let mut out = [0u16; 128];
    for (i, u) in s.encode_utf16().take(127).enumerate() {
        out[i] = u;
    }
    out
}

fn data(hwnd: HWND, icon: HICON, tip: &str) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        szTip: tip_text(tip),
        ..Default::default()
    }
}

pub fn add(hwnd: HWND, icon: HICON, tip: &str) -> bool {
    // SAFETY: `data` is fully initialised.
    unsafe { Shell_NotifyIconW(NIM_ADD, &data(hwnd, icon, tip)).as_bool() }
}

pub fn update(hwnd: HWND, icon: HICON, tip: &str) {
    // SAFETY: as above.
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data(hwnd, icon, tip));
    }
}

pub fn remove(hwnd: HWND) {
    let d = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    };
    // SAFETY: as above.
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &d);
    }
}

pub fn tooltip(mode: InputMode, paused: bool) -> &'static str {
    match (paused, mode) {
        (true, _) => "pKey - tạm dừng",
        (false, InputMode::Vietnamese) => "pKey - Tiếng Việt (Ctrl+Shift để đổi)",
        (false, InputMode::English) => "pKey - English (Ctrl+Shift để đổi)",
    }
}

/// What the menu shows, read from the configuration.
pub struct MenuState {
    pub mode: InputMode,
    pub paused: bool,
    pub autostart: bool,
    pub scheme: Scheme,
    pub tone_style: ToneStyle,
    pub spell_check: bool,
    pub auto_restore: bool,
    pub free_marking: bool,
    pub macros: bool,
}

impl MenuState {
    pub fn new(config: &Config, mode: InputMode, paused: bool, autostart: bool) -> MenuState {
        MenuState {
            mode,
            paused,
            autostart,
            scheme: config.general.scheme,
            tone_style: config.typing.tone_style,
            spell_check: config.typing.spell_check,
            auto_restore: config.typing.auto_restore_invalid,
            free_marking: config.typing.free_marking,
            macros: config.macros.enabled,
        }
    }
}

/// The menu entries as (id, label, checked). `None` id = separator. Pure, for tests.
pub fn menu_items(s: &MenuState) -> Vec<(Option<u32>, &'static str, bool)> {
    let v = |id: u32, label: &'static str, on: bool| (Some(id), label, on);
    vec![
        v(
            IDM_TOGGLE,
            if s.mode == InputMode::Vietnamese {
                "Đang gõ tiếng Việt (bấm để chuyển sang English)"
            } else {
                "Đang gõ English (bấm để chuyển sang tiếng Việt)"
            },
            false,
        ),
        v(IDM_SETTINGS, "Cài đặt...", false),
        (None, "", false),
        v(IDM_TELEX, "Kiểu gõ: Telex", s.scheme == Scheme::Telex),
        v(IDM_VNI, "Kiểu gõ: VNI", s.scheme == Scheme::Vni),
        v(IDM_VIQR, "Kiểu gõ: VIQR", s.scheme == Scheme::Viqr),
        (None, "", false),
        v(
            IDM_MODERN,
            "Đặt dấu kiểu mới (hoà, thuý)",
            s.tone_style == ToneStyle::Modern,
        ),
        v(
            IDM_TRADITIONAL,
            "Đặt dấu kiểu cũ (hòa, thúy)",
            s.tone_style == ToneStyle::Traditional,
        ),
        (None, "", false),
        v(IDM_SPELL, "Kiểm tra chính tả", s.spell_check),
        v(
            IDM_RESTORE,
            "Tự khôi phục từ không phải tiếng Việt",
            s.auto_restore,
        ),
        v(IDM_FREE_MARKING, "Bỏ dấu tự do", s.free_marking),
        v(IDM_MACROS, "Gõ tắt (macro)", s.macros),
        (None, "", false),
        v(IDM_PAUSE, "Tạm dừng", s.paused),
        v(IDM_AUTOSTART, "Khởi động cùng Windows", s.autostart),
        (None, "", false),
        v(IDM_OPEN_DIR, "Mở thư mục cấu hình", false),
        v(IDM_OPEN_CONFIG, "Sửa file cấu hình (config.toml)", false),
        (None, "", false),
        v(IDM_EXIT, "Thoát", false),
    ]
}

/// Show the menu at the cursor and return the chosen command (0 = none).
pub fn show_menu(hwnd: HWND, state: &MenuState) -> u32 {
    // SAFETY: Win32 menu calls; the menu is destroyed before returning.
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return 0;
        };
        for (id, label, checked) in menu_items(state) {
            match id {
                None => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
                Some(id) => {
                    let flags = MF_STRING | if checked { MF_CHECKED } else { MF_UNCHECKED };
                    let _ = AppendMenuW(menu, flags, id as usize, &HSTRING::from(label));
                }
            }
        }
        let _ = MF_POPUP;
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Without this the menu does not close when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, Default::default(), Default::default());
        let _ = DestroyMenu(menu);
        cmd.0 as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> MenuState {
        MenuState::new(&Config::default(), InputMode::Vietnamese, false, false)
    }

    #[test]
    fn menu_reflects_the_configuration() {
        let items = menu_items(&state());
        let checked = |id: u32| items.iter().find(|i| i.0 == Some(id)).unwrap().2;
        assert!(checked(IDM_TELEX) && !checked(IDM_VNI) && !checked(IDM_VIQR));
        assert!(checked(IDM_MODERN) && !checked(IDM_TRADITIONAL));
        assert!(checked(IDM_SPELL) && checked(IDM_RESTORE) && checked(IDM_FREE_MARKING));
        assert!(checked(IDM_MACROS) && !checked(IDM_PAUSE) && !checked(IDM_AUTOSTART));
    }

    #[test]
    fn menu_ids_are_unique() {
        let items = menu_items(&state());
        let mut ids: Vec<u32> = items.iter().filter_map(|i| i.0).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn the_icon_can_be_drawn() {
        let v = make_icon("V", (200, 30, 30));
        let e = make_icon("E", (30, 80, 200));
        let vi = make_icon("VI", (225, 29, 72));
        let en = make_icon("EN", (37, 99, 235));
        assert!(v.is_some() && e.is_some() && vi.is_some() && en.is_some());
    }
}
