# 2026-10-15 — Text service Windows (TSF), P2 phần lõi

**Phase:** P2 (WIN-002a…e, WIN-003 một phần). Căn cứ: `SPEC_pKey_Full_Build.md` §12, §14.1, §15, §16.3.

## Kết quả ngắn gọn
Crate `platform/windows-tsf` (`pkey_tsf.dll`, x64 và x86) cài được như một TIP bàn phím, soạn tiếng Việt bằng composition TSF có gạch chân, và đã được kiểm chứng **tự động trên TSF thật** bằng text store giả (xem "Kiểm thử"). **Chưa từng chạy trong một ứng dụng thật** (Notepad, Chrome…): việc đó cần người dùng cài bằng `scripts/windows/pkey-dev.ps1 install` và thử tay, xem "Việc cần người thử".

## Cấu trúc
| File | Vai trò |
|---|---|
| `platform/windows-tsf/src/keymap.rs` | Thuần, test mọi nơi: `lParam` → scan code → HID usage (hotkey không phụ thuộc layout), VK → `LogicalKey`, dựng `KeyInput` |
| `src/com/mod.rs` | Xuất `DllGetClassObject`, `DllCanUnloadNow`, `DllRegisterServer/Unregister`, `DllInstall` (`regsvr32 /n /i:user`), `guard()` chặn panic không tràn vào host |
| `src/com/registry.rs` | CLSID + `InprocServer32` (HKLM hoặc HKCU), đăng ký profile vi-VN và 5 category TSF. Không ép làm bàn phím mặc định |
| `src/com/service.rs` | `TextService`: `ITfTextInputProcessorEx`, key sink, thread-mgr sink, composition sink, text-edit sink, display-attribute provider |
| `src/com/session.rs` | Edit session đồng bộ (đọc/ghi) và "ngay khi có thể" |
| `src/com/display.rs` | Thuộc tính gạch chân (underline đơn) |
| `ime-runtime` | Mọi quyết định (engine, profile, macro, hotkey, reload) nằm ở đây, dùng chung với C ABI |

## Quyết định thiết kế đáng nhớ
- **Hai nửa của một phím.** TSF gọi `OnTestKey*` rồi mới `OnKey*`, và `OnKey*` chỉ được gọi nếu nửa test trả TRUE. Vì vậy: nửa test nạp phím vào hotkey matcher đúng một lần (kể cả phím modifier và phím nhả) và trả TRUE khi phím bị nuốt **hoặc** phát sinh action (Space commit từ đang gõ rồi vẫn chuyển cho ứng dụng). Quyết định được giữ lại trong `Pending` cho nửa thực thi. Runtime tách `feed_hotkeys` / `run_hotkeys` / `process_engine` cho việc này.
- **Hotkey modifier-only** (`Ctrl+Shift`) bắn khi nhả modifier cuối, không bắn nếu có phím khác xen giữa (đã test cả `Ctrl+Shift+T`).
- **Chế độ VI/EN dùng chung** (`mode_scope = global`) qua *global compartment* TSF với GUID riêng (không dùng `KEYBOARD_OPENCLOSE` để khỏi đổi chỉ báo của Windows). Ghi khi bật/tắt; đọc ở `OnSetFocus` **và** ở đầu mỗi từ, vì trong test (không có cửa sổ thật) TSF không gọi `OnSetFocus` của thread manager. `per_app` nhớ theo `AppIdentity::app_key`.
- **Ô mật khẩu**: đọc `GUID_PROP_INPUTSCOPE` bằng `GetAppProperty` trong edit session đọc, ở đầu mỗi từ; `IS_PASSWORD`/`IS_PRIVATE` ⇒ `Capabilities::SECURE_INPUT` ⇒ engine bỏ qua mọi phím. Bổ sung: kiểm style `ES_PASSWORD` của control Win32 `Edit`/`RichEdit`.
- **Click chuột sang chỗ khác khi đang gõ**: `ITfTextEditSink::OnEndEdit` thấy selection đổi mà không phải do ta ⇒ engine quên từ ngay, composition TSF được kết thúc bằng edit session bất đồng bộ (`TerminateComposition` không dùng được trong lúc TSF giữ lock — đã thử, trả E_FAIL).
- **Composition do host kết thúc** (`OnCompositionTerminated`) ⇒ `Notify::CompositionTerminatedByHost`, engine bỏ trạng thái và không phát action.
- **Panic**: mọi callback COM và edit session chạy trong `catch_unwind`, trả lỗi/FALSE thay vì unwind vào ứng dụng.

## Lỗi/khác biệt đáng ghi
- `GetWindowLongPtrW` trả `isize` trên x64 nhưng `i32` trên x86; chỉ lộ ra khi build 32-bit. Đã sửa (`as u32`).
- `ITfKeystrokeMgr::AdviseKeyEventSink` trả `E_INVALIDARG` cho bất kỳ client id nào không thuộc một TIP được TSF kích hoạt thật. Để test không cần đăng ký IME, feature Cargo `test-hooks` (tắt mặc định) cho phép `ActivateEx` bỏ qua đúng lỗi này. Bản phát hành không có hook, và feature này cũng bật `trace!` (stderr).
- Dòng `warning: linker stdout: ... LNK4104: export of symbol ... should be PRIVATE` khi link DLL là vô hại (hàm xuất theo tên), chưa tắt được gọn.

## Kiểm thử (chạy được, đã chạy)
- `keymap` (7 test), chạy mọi OS.
- `tests/com_smoke.rs`: nạp DLL thật bằng `LoadLibrary`, kiểm export, chỉ phục vụ đúng CLSID, `DllInstall` từ chối đối số lạ **trước khi** ghi gì, mọi interface TSF cần (`QueryInterface`), thuộc tính gạch chân, đếm tham chiếu (`DllCanUnloadNow` về `S_OK`).
- `tests/composition.rs`: **TSF thật** (`ITfThreadMgr`, `ITfContext`, composition, range, property) + `ITextStoreACP` giả trong bộ nhớ, gọi trực tiếp key sink của text service. 8 kịch bản: gõ `tieengs`/`vieejt` ra `tiếng việt` qua `SetText` trên range composition; Backspace; từ không phải tiếng Việt (`windows`) được khôi phục; host tự kết thúc composition; `Ctrl+Shift` bật/tắt; bật/tắt commit từ đang gõ; `Ctrl+Shift+T` không bật/tắt; click đi chỗ khác kết thúc composition; chế độ dùng chung giữa hai tài liệu; ô mật khẩu qua input scope.
- Cả hai bộ chạy được trong **tiến trình 32-bit** (`cargo test -p pkey-tsf --target i686-pc-windows-msvc`): TIP x86 nạp được và gõ đúng trong tiến trình 32-bit của test (chưa phải ứng dụng 32-bit của bên thứ ba).
- Test không ghi registry, không đăng ký bộ gõ. `PKEY_REQUIRE_TSF_TESTS=1` biến "bỏ qua" thành lỗi. CI chưa bật biến này vì chưa biết runner Windows có cho tạo `ITfThreadMgr` hay không.

## Chưa làm / rủi ro còn mở (WIN-002/003 chưa xong)
- **Chưa thử trong ứng dụng thật.** Text store giả đơn giản hơn nhiều so với Chrome/Word/VS Code (không có `GetTextExt`, không có layout, không có edit transaction thật); lỗi tương thích chỉ lộ ra khi thử trên máy thật. `QA-W1` (Notepad, Chrome, VS Code, Word) chưa chạy; gate G2 chưa đạt.
- Đường đăng ký (`registry.rs`: ghi CLSID, `RegisterProfile`, `RegisterCategory`) **chưa được chạy** vì sẽ cài bộ gõ lên máy dev; chỉ biên dịch được. Phải chạy `pkey-dev.ps1 install` để kiểm. Biểu tượng profile chưa có (tài nguyên icon trong DLL chưa tạo).
- Chưa có: chỉ báo VI/EN trên language bar (`ITfLangBarItemButton`) và tray (UI-000, cần IPC-001a); `ITfCompartmentEventSink` (hiện chỉ đọc khi lấy focus/đầu từ); `ITfKeystrokeMgr::PreserveKey` cho hotkey chord (chord được xử lý trong key sink); định danh AUMID cho ứng dụng UWP; cấu hình ACL `ALL APPLICATION PACKAGES` cho thư mục config để app UWP/AppContainer đọc được (WIN-003) — app AppContainer sẽ **không** đọc được `%APPDATA%\pKey` và dùng cấu hình mặc định.
- Dùng `Runtime::from_default_paths()` ở `static`, tức là mỗi tiến trình đọc file config riêng và tự reload khi `mtime` đổi; chưa có snapshot/generation counter như §14.1.
- `OpenSettings` hotkey chưa làm gì (chưa có ứng dụng Settings). `CycleScheme` chỉ đổi trong bộ nhớ của tiến trình hiện tại, không ghi file và không lan sang tiến trình khác.
- Thất bại edit session chỉ đặt lại trạng thái (`HostCommitFailed`), chưa có thử lại.
- x86: build và test pass; ARM64 chưa làm.

## Việc cần người thử (trên máy thật)
1. `powershell -ExecutionPolicy Bypass -File scripts\windows\pkey-dev.ps1 install` (không cần quyền admin, chỉ ghi HKCU và `%LOCALAPPDATA%\pKey\bin`).
2. Settings › Time & language › Language & region › Vietnamese › Language options › Add a keyboard › pKey; chọn pKey trên thanh tác vụ (Win+Space).
3. Thử Notepad, Chrome (ô địa chỉ và Google Docs), VS Code, Word; gõ `tieengs vieejt`, Backspace giữa chừng, `Ctrl+Shift` đổi VI/EN, click chuột giữa từ, ô mật khẩu.
4. Gỡ: `... pkey-dev.ps1 uninstall`.
Báo lại ứng dụng nào lỗi và cách tái hiện; đó là thông tin cần nhất cho bước tiếp theo.

## Cách chạy
```
cargo test -p pkey-tsf                                      # x64
cargo test -p pkey-tsf --target i686-pc-windows-msvc        # x86 (rustup target add i686-pc-windows-msvc)
cargo build -p pkey-tsf --release [--target i686-pc-windows-msvc]
```

## Bản cài đặt (REL-000, bản dev)
`installer/windows/pkey.iss` (Inno Setup 6) + `scripts/windows/build-installer.ps1` tạo `target/installer/pKey-Setup-<version>.exe` (~2,8 MB) và `.sha256`. Theo từng người dùng, không cần admin: chép DLL x64/x86 và `pkey.exe` vào `%LOCALAPPDATA%\pKey`, chạy `regsvr32 /n /i:user` cho từng bản; gỡ thì hủy đăng ký. Khi nâng cấp, DLL cũ được hủy đăng ký trước; DLL đang bị ứng dụng giữ khóa sẽ được thay lúc khởi động lại. Không xóa `config.toml`/`macros.json` khi gỡ.
**Đã kiểm:** build thành công. **Chưa kiểm:** chạy bộ cài, nâng cấp, gỡ trên máy sạch (sẽ cài bộ gõ lên máy dev); chưa ký Authenticode; chưa có icon; chưa có bản toàn máy (HKLM) và chưa có MSI.
