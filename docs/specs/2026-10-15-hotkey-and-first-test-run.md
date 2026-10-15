# 2026-10-15 — Hotkey parser/matcher (CORE-007) và lần chạy test đầu tiên

## Context / Why
Máy đã có linker MSVC, nên lần đầu chạy được `cargo test` (spec trước, `2026-10-15-core-engine-scaffold.md`, ghi "chưa chạy test nào"). Đồng thời làm tiếp work package CORE-007 trong `IMPLEMENTATION_PHASES.md` (P1), không phụ thuộc OS và không cần thư viện mới.

## Kết quả chạy test đầu tiên
- Toàn workspace: tất cả pass, golden `telex.golden` + `vni.golden` = **120/120 case pass**, `pkey-cli simulate "tieengs vieejt "` ra `tiếng việt `.
- Chỉ có 1 lỗi, và là **lỗi dữ liệu test, không phải lỗi engine**: `ime-vietnamese::tests::valid_syllables` truyền `"người"` (đã có dấu huyền) vào `letters_from_display`. Hàm này chỉ nhận chữ chưa có dấu thanh (dấu thanh nằm riêng trong `Tone`), nên `ờ` bị coi là ký tự ngoài `a-z` → `Invalid`. Sửa đầu vào thành `"ngươi"`, đúng với quy ước của các ca còn lại. Không đổi logic `validity`.
- `cargo clippy --workspace --all-targets -- -D warnings` và `cargo fmt --check` sạch.

## CORE-007 — thay đổi
- Mới: `crates/ime-core/src/hotkey.rs`, export `Hotkey`, `HotkeyError`, `HotkeyMatcher` từ `ime-core`.
- `Hotkey: FromStr + Display` theo cú pháp §16.3 `Mod(+Mod)*(+Key)?`. Không phân biệt hoa/thường; alias: `Control`, `Option`, `Win`/`Cmd`/`Super`, `Esc`, `Return`. `Display` in dạng chuẩn (`Ctrl, Shift, Alt, Meta` rồi phím), parse lại ra cùng giá trị.
- Phím so khớp theo `physical_code` = HID usage page 0x07 (A–Z `0x04..`, 1–0, F1–F24, Space, Enter, mũi tên…), không phụ thuộc layout.
- Lỗi parse có kiểu riêng: rỗng, token rỗng, phím lạ, modifier trùng, nhiều phím, **thiếu modifier** (phím trần như `A` bị từ chối vì sẽ nuốt ký tự gõ thường).
- `HotkeyMatcher::feed(&KeyInput) -> bool`:
  - `Mod+Key`: bắn khi nhấn phím (bỏ qua auto-repeat) và tập modifier đang giữ **đúng bằng** tập của hotkey.
  - Modifier-only (`Ctrl+Shift`): bắn khi **nhả modifier cuối cùng**, với điều kiện giữa chừng không có phím không-modifier nào và tập modifier cao nhất đã giữ đúng bằng hotkey. Vì vậy `Ctrl+Shift+T` và `Ctrl+Shift+Alt` không kích hoạt.
  - Matcher tự theo dõi modifier từ mã HID `0xE0–0xE7` (không dùng `KeyInput::modifiers`, vì snapshot đó không cho biết modifier nào nhả sau cùng). Adapter phải đưa cả sự kiện nhấn/nhả của phím modifier vào matcher. `reset()` dùng khi đổi focus.

## Hạn chế còn mở
- Modifier trái/phải được gộp: nhả Ctrl trái khi Ctrl phải còn giữ coi như nhả Ctrl. Hiếm gặp, chỉ làm chord kết thúc sớm.
- Alt phải được coi là Alt (trên layout có AltGr thì adapter cần lọc theo `Modifiers::ALT_GR` nếu muốn khác).
- Chưa nối vào `Engine`/config: engine chưa gọi matcher, `ime-config` chưa có nên chưa có trường hotkey. Chưa có phát hiện xung đột với phím hệ điều hành (thuộc UI-002a).
- Chưa kiểm chứng với sự kiện phím thật; chỉ test bằng chuỗi sự kiện giả.

## How to test
```
cargo test -p ime-core          # 21 test, gồm 8 test hotkey
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
Chạy trong PowerShell, không dùng Git Bash (xem spec scaffold).
