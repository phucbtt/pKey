//! Typing into the application with `SendInput`.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK,
};

use crate::controller::Op;

/// A key the hook swallowed and now replays after the edits.
#[derive(Clone, Copy, Debug)]
pub struct Replay {
    pub vk: u32,
    pub scan: u32,
    pub extended: bool,
}

fn key(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// The key events for `ops`. Pure, so it can be tested.
pub fn build_inputs(ops: &[Op], replay: Replay) -> Vec<INPUT> {
    let mut v = Vec::new();
    for op in ops {
        match op {
            Op::Backspaces(n) => {
                for _ in 0..*n {
                    v.push(key(VK_BACK.0, 0, KEYBD_EVENT_FLAGS(0)));
                    v.push(key(VK_BACK.0, 0, KEYEVENTF_KEYUP));
                }
            }
            Op::Text(t) => {
                // Unicode packets: the character itself, whatever the keyboard layout is.
                for unit in t.encode_utf16() {
                    v.push(key(0, unit, KEYEVENTF_UNICODE));
                    v.push(key(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                }
            }
            Op::ClearAutocomplete => {
                // U+202F (narrow no-break space) replaces a selected suggestion; Backspace then
                // removes it again, leaving the real text as it was.
                v.push(key(0, 0x202F, KEYEVENTF_UNICODE));
                v.push(key(0, 0x202F, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                v.push(key(VK_BACK.0, 0, KEYBD_EVENT_FLAGS(0)));
                v.push(key(VK_BACK.0, 0, KEYEVENTF_KEYUP));
            }
            Op::ReplayKey => {
                let ext = if replay.extended {
                    KEYEVENTF_EXTENDEDKEY
                } else {
                    KEYBD_EVENT_FLAGS(0)
                };
                v.push(key(replay.vk as u16, replay.scan as u16, ext));
                v.push(key(
                    replay.vk as u16,
                    replay.scan as u16,
                    ext | KEYEVENTF_KEYUP,
                ));
            }
        }
    }
    v
}

/// Send `ops`. Returns whether every event was accepted (it is not when a higher-privilege
/// window has the focus).
pub fn send(ops: &[Op], replay: Replay) -> bool {
    let inputs = build_inputs(ops, replay);
    if inputs.is_empty() {
        return true;
    }
    // SAFETY: `inputs` is a valid slice of fully initialised INPUT structures.
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    sent as usize == inputs.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(i: &INPUT) -> u32 {
        unsafe { i.Anonymous.ki.dwFlags.0 }
    }

    #[test]
    fn backspaces_and_text_become_key_pairs() {
        let r = Replay {
            vk: 0x20,
            scan: 0x39,
            extended: false,
        };
        let v = build_inputs(&[Op::Backspaces(2), Op::Text("ếa".into())], r);
        assert_eq!(v.len(), 4 + 4);
        // down, up for Backspace
        assert_eq!(unsafe { v[0].Anonymous.ki.wVk.0 }, VK_BACK.0);
        assert_eq!(flags(&v[1]) & KEYEVENTF_KEYUP.0, KEYEVENTF_KEYUP.0);
        // Unicode packets carry the UTF-16 unit in wScan
        assert_eq!(unsafe { v[4].Anonymous.ki.wScan }, 'ế' as u16);
        assert_eq!(flags(&v[4]) & KEYEVENTF_UNICODE.0, KEYEVENTF_UNICODE.0);
    }

    #[test]
    fn characters_outside_the_bmp_use_surrogate_pairs() {
        let r = Replay {
            vk: 0,
            scan: 0,
            extended: false,
        };
        let v = build_inputs(&[Op::Text("😀".into())], r);
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn the_replayed_key_keeps_its_extended_flag() {
        let r = Replay {
            vk: 0x0D,
            scan: 0x1C,
            extended: true,
        };
        let v = build_inputs(&[Op::ReplayKey], r);
        assert_eq!(v.len(), 2);
        assert_eq!(
            flags(&v[0]) & KEYEVENTF_EXTENDEDKEY.0,
            KEYEVENTF_EXTENDEDKEY.0
        );
        assert_eq!(unsafe { v[0].Anonymous.ki.wVk.0 }, 0x0D);
    }

    #[test]
    fn the_autocomplete_fix_is_an_invisible_character_and_a_backspace() {
        let r = Replay {
            vk: 0,
            scan: 0,
            extended: false,
        };
        let v = build_inputs(&[Op::ClearAutocomplete], r);
        assert_eq!(v.len(), 4);
        assert_eq!(unsafe { v[0].Anonymous.ki.wScan }, 0x202F);
        assert_eq!(unsafe { v[2].Anonymous.ki.wVk.0 }, VK_BACK.0);
    }

    #[test]
    fn nothing_to_do_builds_nothing() {
        let r = Replay {
            vk: 0,
            scan: 0,
            extended: false,
        };
        assert!(build_inputs(&[], r).is_empty());
    }
}
