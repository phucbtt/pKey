# 2026-10-15 — App độc lập kiểu EVKey (`pkey.exe`)

## Yêu cầu
Một chương trình chạy ở khay hệ thống, gõ tiếng Việt vào **mọi** ứng dụng mà không phải thêm bàn phím trong Windows Settings (như EVKey). Bản TSF không làm được điều này, nên đây là một đường thứ hai dùng chung lõi (`ime-runtime`). Trong spec gốc, đây là "hook fallback" F014 (dự kiến 1.1); nó được đưa lên làm đường chính theo yêu cầu.

## Cách hoạt động
- **Hook `WH_KEYBOARD_LL`** thấy mỗi phím trước ứng dụng. Với mỗi phím, `Controller` (thuần, không Win32) trả về: có nuốt phím không và các thao tác cần chèn.
- **Không có composition của TSF**: chữ được gõ thẳng vào ứng dụng và sửa dần. Mỗi lần từ đổi, app tính phần khác nhau ngắn nhất (`edit_ops`): `Backspace × n` + gõ phần mới bằng gói Unicode của `SendInput` (không phụ thuộc layout). Ví dụ `tiêng` → `tiếng` = 3 Backspace + `ếng`.
- Phím không được engine tiêu thụ nhưng làm đổi từ (Space sau từ phải khôi phục, ví dụ `windows`) bị nuốt rồi **phát lại** sau khi sửa từ.
- **Chống vòng lặp**: mọi phím do `SendInput` tạo ra (cờ `LLKHF_INJECTED`) đều bị bỏ qua.
- **Ngữ cảnh**: mỗi ô nhập (cửa sổ đang giữ focus) có một context engine riêng. Đổi cửa sổ hoặc **click chuột** (hook chuột) thì quên từ đang gõ, không đụng vào văn bản.
- **Chế độ VI/EN dùng chung** cho mọi cửa sổ (`mode_scope`), bật/tắt bằng `Ctrl+Shift` (modifier-only, không bắn nếu có phím khác xen giữa), bằng click vào icon khay, hoặc menu. Icon: ô đỏ "V" / ô xanh "E".
- **Menu khay**: chuyển VI/EN, kiểu gõ (Telex/VNI/VIQR), kiểu đặt dấu, kiểm tra chính tả, tự khôi phục, bỏ dấu tự do, macro, tạm dừng, khởi động cùng Windows, mở thư mục/file cấu hình, thoát. Thay đổi được lưu vào `config.toml` ngay (atomic) và áp dụng tức thì; sửa file tay cũng tự nạp lại khi đổi cửa sổ.
- Profile theo ứng dụng và game mode (`config.toml`) dùng được vì app biết `exe_name` của cửa sổ đang focus. Ô mật khẩu của control Win32 (`Edit` có `ES_PASSWORD`) được bỏ qua.
- Một bản duy nhất chạy một lúc (mutex `Local\pKey.App`).

## Cấu trúc
| Thành phần | Vai trò |
|---|---|
| `crates/ime-winkeys` | Bảng phím Windows (scan code → HID, VK → phím logic), dùng chung với TSF |
| `platform/windows-app/src/controller.rs` | Quyết định, thuần, 16 test |
| `platform/windows-app/src/win/inject.rs` | Dựng/gửi `SendInput` |
| `platform/windows-app/src/win/sys.rs` | Cửa sổ đang focus, exe, ký tự của phím theo layout, ô mật khẩu, tự khởi động (HKCU Run) |
| `platform/windows-app/src/win/tray.rs` | Vẽ icon V/E bằng GDI, menu |
| `platform/windows-app/src/win/mod.rs` | Cửa sổ ẩn, hook phím/chuột, `SetWinEventHook` đổi foreground, vòng lặp thông điệp |
| `Runtime::update_config` | Đổi cấu hình từ UI, áp dụng và lưu |

## Kiểm thử
- 16 test của `controller` (một "ứng dụng" giả áp các thao tác và phím không bị nuốt): gõ `tieengs vieejt `, Backspace chỉ sửa từ, Esc trả phím thô, khôi phục từ không phải tiếng Việt, `Ctrl+Shift`, chia sẻ chế độ giữa cửa sổ, đổi cửa sổ/click quên từ, ô mật khẩu, tạm dừng, game mode, macro.
- Test `inject` (dựng INPUT) và `tray` (menu, vẽ icon GDI thật).
- `--selftest` (debug và release): tạo cửa sổ, gắn hook, thêm icon khay, thoát sạch.
- **E2E thật** (`tests/e2e.rs`, chỉ chạy khi `PKEY_RUN_E2E=1`, cần `--features test-hooks`): app thật trên một luồng, một ô `EDIT` Windows thật trên luồng khác, phím thật qua `SendInput` → hook → engine → `SendInput` → ô soạn thảo. Đã PASS: `tiếng việt `, Backspace giữa từ (`tô `), `windows` được khôi phục, `Ctrl+Shift` sang English rồi về. Test chỉ gõ khi cửa sổ của nó thật sự giữ bàn phím, và có watchdog 40 giây để không bao giờ để hook toàn cục treo lại. (Một lần chạy đầu bị treo vì luồng cửa sổ test không thoát; đã sửa.)

## Hạn chế đã biết (chưa kiểm trên ứng dụng thật ngoài ô EDIT)
- Hoạt động bằng Backspace + chèn Unicode nên **không hợp** với: ứng dụng/terminal/game bỏ qua gói Unicode hoặc xử lý phím chậm, Remote Desktop/VM tùy cấu hình, ô địa chỉ trình duyệt có gợi ý tự điền (có thể xóa nhầm gợi ý), Word/Office khi autocorrect chen vào. Đây là điểm yếu chung của mọi bộ gõ kiểu hook; cần thử trên máy thật và có thể phải thêm danh sách ứng dụng dùng cách khác (chọn rồi thay bằng Shift+Left).
- **Cửa sổ chạy quyền admin**: hook của app chạy quyền thường sẽ không chèn được vào đó (UIPI). Cần chạy app bằng quyền admin; chưa có tùy chọn/nhắc.
- Ô mật khẩu của trình duyệt/UWP (UI Automation) **chưa** được nhận ra; chỉ nhận control Win32 `Edit/RichEdit` có `ES_PASSWORD`. Có thể bấm "Tạm dừng" hoặc dùng profile theo ứng dụng.
- Không dùng chung với bàn phím TSF "pKey": nếu chọn cả hai, mỗi phím bị xử lý hai lần. Bộ cài đặt TSF thành thành phần tùy chọn, không chọn mặc định.
- Bảng mã chỉ Unicode dựng sẵn (NFC) được kiểm; NFD có thể xóa sai số ký tự ở vài ứng dụng. TCVN3/VNI-Windows (1.1) chưa có.
- Chưa có: cửa sổ Settings đầy đủ (hiện dùng menu khay và sửa `config.toml`), cấu hình hotkey bằng UI, quản lý macro bằng UI, âm báo, danh sách ứng dụng loại trừ bằng UI, icon `.exe`/icon cài đặt, chữ ký số.
- Chưa có chế độ "ghi nhớ VI/EN theo ứng dụng" trên menu (có trong config: `mode_scope = "per_app"`).
- Hook phím đồng bộ trên luồng giao diện: nếu luồng bận hơn ~300 ms Windows sẽ gỡ hook (hiếm, vì xử lý chỉ vài micro giây).

## Chạy
```
cargo run -p pkey-app --release          # hoặc cài bằng pKey-Setup-<ver>.exe
cargo test -p pkey-app                   # controller, inject, tray
PKEY_RUN_E2E=1 cargo test -p pkey-app --features test-hooks --test e2e -- --nocapture
```

## Sửa lỗi: thanh địa chỉ trình duyệt (caách thay vì cách)
Thanh địa chỉ Chrome/Edge/Firefox tự điền phần gợi ý (được bôi chọn) sau chữ đang gõ. Khi app đổi ca thành cá, Backspace đầu tiên xóa phần gợi ý chứ không xóa chữ \a, nên ra ca + á. Sửa: trước Backspace đầu tiên, nếu ứng dụng là trình duyệt (chrome/msedge/brave/vivaldi/opera/firefox/arc/chromium.exe) thì gõ ký tự U+202F rồi xóa nó (Op::ClearAutocomplete), việc này dập gợi ý mà không đổi văn bản. Chỉ thêm khi có Backspace; thao tác chỉ nối chữ không cần. Test: \browsers_get_the_autocomplete_fix_only_before_backspaces, 	he_autocomplete_fix_is_an_invisible_character_and_a_backspace. **Chưa kiểm trên Chrome thật** (test e2e chỉ dùng ô EDIT); cần người dùng thử lại.

## Cửa sổ Cài đặt (như EVKey)
Mở bằng menu khay "Cài đặt...", nháy đúp icon khay, hoặc hotkey open_settings; cờ --settings[=N] mở thẳng tab N. Win32 thuần (không thêm thư viện), 4 tab: **Chung** (kiểu gõ Telex/VNI/VIQR, bảng mã, kiểu đặt dấu, 10 tùy chọn gồm chính tả, tự khôi phục, bỏ dấu tự do, Esc, Backspace theo phím, w→ư, [ ], phụ âm f j w z, macro, khởi động cùng Windows), **Phím tắt** (Ctrl/Shift/Alt/Win + phím thêm, mặc định Ctrl+Shift), **Gõ tắt** (thêm/cập nhật/xóa, lưu vào macros.json), **Ứng dụng** (danh sách chương trình mà pKey không làm gì, lưu thành profile game mode có tiền tố tên Tắt pKey:  nên không đụng profile viết tay). OK/Áp dụng kiểm tra rồi áp dụng ngay và lưu atomic; Hủy không lưu.

Cấu trúc: settings_model.rs (thuần, 9 test: đọc/ghi cấu hình, hotkey, profile loại trừ), win/settings.rs (cửa sổ), Runtime::update_macros. Giao diện hiện đại (comctl32 v6) qua /MANIFESTDEPENDENCY trong \build.rs. Kiểm thử: 	ests/settings_ui.rs (chỉ chạy khi PKEY_RUN_E2E=1) mở cửa sổ thật, bấm nút, xác nhận config.toml/macros.json, mở lại, Hủy; đã PASS. Đã chụp màn hình các tab để kiểm bố cục.
Chưa có: chọn chương trình từ danh sách đang chạy, nhập/xuất CSV macro, chỉnh expand_on, bản dịch tiếng Anh, chế độ tối.


## Đổi tên thành pKey (toàn dự án)
Sản phẩm trước đây tên **VietIME**; toàn bộ tên được đổi sang **pKey** (mã, file, tài liệu, CI, bộ cài).

| Trước | Sau |
|---|---|
| app `vietime-app.exe` | `pkey.exe` |
| CLI `vietime.exe` | `pkey-cli.exe` |
| package `vietime-app`, `vietime-tsf` | `pkey-app`, `pkey-tsf` (các crate `ime-*` giữ nguyên, là tên chung của lõi) |
| `vietime_tsf.dll`, `vietime_ffi` | `pkey_tsf.dll`, `pkey_ffi` |
| `include/vietime.h`, `VIETIME_H` | `include/pkey.h`, `PKEY_H` (tiền tố hàm C `vt_`/`VT_` giữ nguyên) |
| `installer/windows/vietime.iss`, `VietIME-Setup-<ver>.exe` | `pkey.iss`, `pKey-Setup-<ver>.exe` |
| `scripts/windows/vietime-dev.ps1` | `pkey-dev.ps1` |
| `docs/SPEC_VietIME_*.md` | `docs/SPEC_pKey_*.md` |
| biến môi trường `VIETIME_*` (`CONFIG_DIR`, `RUN_E2E`, `REQUIRE_*`, `C_SANITIZE`) | `PKEY_*` |
| `CLSID_VIETIME`, lớp cửa sổ `VietIME.*`, mutex | `CLSID_PKEY`, `pKey.*` |
| thư mục `%APPDATA%\VietIME` (macOS `Application Support/VietIME`, Linux `~/.config/vietime`) | `...\pKey` (Linux `pkey`) |

**Giữ nguyên có chủ đích**: các GUID của TSF (CLSID, profile) và `AppId` của bộ cài, để không đăng ký trùng và để bộ cài mới nâng cấp thay bản cũ.

**Tương thích ngược** (có test): (1) lần chạy đầu, thư mục cấu hình/runtime `VietIME` được **chuyển** sang `pKey` nếu `pKey` chưa tồn tại, không bao giờ ghi đè thư mục mới (`ime_config::migrate_dir`); (2) biến `VIETIME_CONFIG_DIR` vẫn được nhận khi `PKEY_CONFIG_DIR` không đặt; (3) mục tự khởi động `Run\VietIME` được nhận ra và thay bằng `Run\pKey`; (4) profile `Tắt VietIME: ...` vẫn được nhận và ghi lại với tiền tố `Tắt pKey: `; (5) bộ cài xóa thư mục cài cũ `%LOCALAPPDATA%\VietIME\bin`, `vietime-app.exe`, `vietime.exe`, lối tắt `VietIME` và mục `Run\VietIME`, và tắt tiến trình `vietime-app.exe` trước khi cài. Bộ cài mới đặt thư mục cài là `%LOCALAPPDATA%\pKey`.

## Bản một file `dist\pKey.exe`
**Yêu cầu:** một file chạy được trên mọi máy Windows, không cài đặt.
**Cách làm** (`scripts/windows/build-portable.ps1`, profile `dist`): build `pkey-app` cho `i686-pc-windows-msvc`, CRT liên kết tĩnh (`.cargo/config.toml`: `+crt-static`), `opt-level = "s"`, LTO đầy đủ, không debug info, `panic = "unwind"` (hook cần `catch_unwind`). Kết quả khoảng 1 MB.
**Vì sao 32-bit:** một chương trình 32-bit chạy trên x86, x64 và ARM64 (giả lập). Hook `WH_KEYBOARD_LL`, `SendInput`, đọc layout và tên tiến trình của cửa sổ 64-bit đều dùng được từ tiến trình 32-bit. Đánh đổi: không có bản native 64-bit/ARM64 (không đáng kể với một chương trình xử lý phím).
**Kiểm tra trong script:** (1) file không chứa tham chiếu `VCRUNTIME/MSVCP/api-ms-win-crt/ucrtbase` (CRT thật sự tĩnh), (2) `--selftest` khởi động, tạo cửa sổ, hook, icon khay và thoát sạch. Bảng import: chỉ KERNEL32, USER32, GDI32, ADVAPI32, SHELL32, COMCTL32, DWMAPI, UXTHEME, OLEAUT32, NTDLL, BCRYPTPRIMITIVES và API set của Windows, tức là DLL có sẵn trong Windows.
**Test xuyên kiến trúc:** `a_separate_pkey_exe_types_into_this_process` chạy file `.exe` 32-bit (bản `--features test-hooks`, cùng mã) như tiến trình riêng và gõ vào cửa sổ của tiến trình test 64-bit: `tiếng việt `, `windows` được khôi phục. Chạy bằng `PKEY_RUN_E2E=1 PKEY_E2E_EXE=<exe>`.
**Hạn chế:** chỉ Windows 10/11 (Rust hiện tại không hỗ trợ Windows 7/8); chưa ký số nên SmartScreen có thể cảnh báo lần đầu ("More info" rồi "Run anyway"); chưa có icon riêng cho file `.exe`; chưa chạy thử trên một máy Windows khác hoàn toàn sạch (chỉ có máy phát triển này); cần quyền admin để gõ vào cửa sổ chạy quyền admin.
**Dọn dẹp cùng đợt:** các test tự xóa thư mục tạm của chúng (`%TEMP%\pkey-*`) thay vì để rác; xóa `scratch/` (script kiểm tra icon dùng một lần); `cargo clean` bỏ ~8,6 GB cache build; `dist/` thêm vào `.gitignore`.
