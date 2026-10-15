# 2026-10-15 — Engine IBus cho Ubuntu (LIN-001, LIN-003 phần lõi)

**Phase:** P3. Căn cứ: `SPEC_pKey_Full_Build.md` §14.4, §14.6, §15, §24.3. Quyết định ADR-004 phương án thay thế: **thuần Rust qua `zbus`**, không dùng C shim.

## Kết quả ngắn gọn
`platform/linux-ibus` (`pkey-ibus`, binary `ibus-engine-pkey`) là engine IBus đầy đủ chức năng gõ: Telex/VNI/VIQR, preedit gạch chân (mode COMMIT), commit khi gặp dấu cách, hotkey `Ctrl+Shift` đổi VI/EN, chỉ báo VI/EN, ô mật khẩu pass-through, cấu hình/macro/profile dùng chung với Windows qua `ime-runtime`. Đã được kiểm chứng **tự động qua một `ibus-daemon` thật** (xem "Kiểm thử"). **Chưa thử tay trong ứng dụng GTK/Qt/Chromium thật trên một phiên desktop**: xem "Việc cần người thử".

## Vì sao chạy được trên Ubuntu cũ và mới
- **Một binary tĩnh** (`x86_64-unknown-linux-musl`, ~2,2 MB): không phụ thuộc glibc, libibus, GLib hay GTK. `ldd` báo "statically linked", không có symbol `GLIBC_*`. Cùng một file chạy trên 20.04, 22.04, 24.04, 26.04 (và Debian).
- **Giao thức nói thẳng qua D-Bus** (zbus) nên không bị ràng buộc phiên bản `libibus`. Các giao diện được đối chiếu với XML mà libibus 1.5.34 mang theo, và kiểu dữ liệu (`IBusText`, `IBusProperty`, `IBusComponent`) được đối chiếu với kết quả serialize của chính libibus.
- **Tương thích ngược với IBus cũ** (1.5.22 trên 20.04, 1.5.26 trên 22.04): daemon cũ không có `FocusInId` thì gọi `FocusIn`, và thuộc tính `FocusId` lạ bị bỏ qua; app nhận danh tính `ibus_client` chỉ khi daemon hỗ trợ. Cả hai đường đều có trong engine.
- **Tìm bus IBus** bằng `IBUS_ADDRESS` hoặc quét `~/.config/ibus/bus/` (tên file khác nhau giữa X11 và Wayland, giữa các phiên bản).
- Wayland và X11 đều đi qua IBus, nên không cần hook toàn cục (spec §3 loại trừ X11 global hook).

## Cấu trúc
| File | Vai trò |
|---|---|
| `src/keymap.rs` | Thuần: `keyval/keycode/state` IBus → `KeyInput` (modifier, keysym → ký tự, evdev → HID cho hotkey không phụ thuộc layout). Tái dùng bảng scan code của `ime-winkeys` (evdev 1–88 = set-1) |
| `src/session.rs` | Thuần: một context = một `Session`; chuyển callback IBus thành lời gọi `Runtime` và output thành `Effect` (commit, preedit, ẩn preedit, đổi chế độ) |
| `src/wire.rs` | `IBusText`, `IBusAttrList`, `IBusProperty`, `IBusPropList`, `IBusComponent` dưới dạng `zvariant::Value` |
| `src/dbus.rs` | `org.freedesktop.IBus.Engine/Factory/Service` bằng zbus; panic được `catch_unwind` và coi như "phím đi qua" (fail-open) |
| `src/address.rs` | Tìm địa chỉ bus |
| `data/pkey.xml.in` | File component cho `/usr/share/ibus/component` |
| `scripts/linux/build-deb.sh`, `install.sh`, `e2e-ibus.sh` | Đóng gói `.deb`, cài không qua gói, test e2e |

## Quyết định đáng nhớ
- **Một `Runtime` cho cả tiến trình**, mỗi đối tượng engine IBus là một `ContextId`. Chế độ VI/EN dùng chung (`mode_scope = global`).
- **Mất focus không phát commit**: preedit mode là COMMIT, nên daemon tự commit preedit của context vừa mất focus (đã kiểm: `focus-out keeps the word`). Engine chỉ quên trạng thái; phát thêm commit sẽ chèn trùng.
- **`Reset` thì commit từ đang gõ** (con trỏ bị đổi bằng chuột), khác với focus-out.
- **Phím nhả không bao giờ bị nuốt**, để trạng thái phím của ứng dụng cân bằng.
- **Ô mật khẩu/PIN** (`ContentType` purpose 8/9) ⇒ `SECURE_INPUT`; terminal (10) ⇒ `TERMINAL_HINT`.
- **`ibus-daemon` chỉ đọc `/usr/share/ibus/component`** (đã kiểm bằng `strace`): không có thư mục component theo người dùng. Việc `RegisterComponent` động cũng không làm engine xuất hiện trong danh sách nếu chưa có file XML. Vì vậy cài đặt cần quyền root một lần (gói `.deb` hoặc `install.sh`); không thể "cài không sudo".
- **Chưa làm**: surrounding text (F018/F020, mặc định tắt), `inotify` (thay bằng `poll_reload` ở mỗi lần focus-in), cửa sổ Settings, `OpenSettings` hotkey, `.rpm`, Fcitx5 (LIN-002/004).

## Kiểm thử
- 23 test đơn vị (`cargo test -p pkey-ibus`): keymap, session (Telex, release, mật khẩu, đổi chế độ, reset, focus, Ctrl+phím), dạng dữ liệu D-Bus, tìm địa chỉ bus.
- **E2E `scripts/linux/e2e-ibus.sh`** (15 kiểm tra, đạt 15/15 với cả bản debug lẫn bản musl phát hành): chạy một `ibus-daemon` riêng trong mount namespace tạm (không cần root; bind-mount thư mục component), daemon tự khởi động engine bằng `--ibus` đúng như khi cài gói. Script Python đóng vai ứng dụng bằng `IBus.InputContext`: chọn engine như GNOME (`set_global_engine`), gõ `tieengs` ra `tiếng`, câu "Việt Nam", Backspace, Enter, `Ctrl+C`, `Ctrl+Shift` đổi VI/EN, ô mật khẩu, hai context độc lập, mất/lấy lại focus. Không động tới phiên IBus thật của người dùng.
- CI: job `linux-ibus` chạy cùng binary tĩnh với IBus của **Ubuntu 22.04 và 24.04**.
- Đã chạy trên máy phát triển: Ubuntu 26.04 (IBus 1.5.34, GNOME Wayland). **Chưa chạy trên máy Ubuntu 20.04/22.04 thật** (không có ở đây); khả năng chạy được dựa vào binary tĩnh, tương thích giao thức và job CI 22.04.

## Gạch chân và chế độ gõ thẳng (`compat.linux_preedit`)
- Desktop (GNOME/Wayland, Chromium, GTK4) **luôn gạch chân chữ đang ở dạng preedit**, bất kể engine có gửi thuộc tính gạch chân hay không (`preedit_underline = false` chỉ bỏ thuộc tính, đã kiểm: engine gửi `None`).
- `linux_preedit = false` gõ thẳng chữ vào ô: mỗi phím gửi `DeleteSurroundingText` rồi `CommitText`. Qua `ibus-daemon` thật thì đúng (3 kiểm tra e2e), nhưng **trên máy người dùng gõ nhanh bị loạn phím**; bật lại preedit thì hết. Nghi nguyên nhân: giao thức text-input-v3 của Wayland gom nhiều cặp xóa/chèn vào một `done`, nên chỉ một phần được áp dụng. Chưa xác minh bằng ứng dụng cụ thể.
- Vì vậy **mặc định là preedit** (`true`); chế độ gõ thẳng là thử nghiệm, chỉ nên bật cho ứng dụng đã thử, và tự lùi về preedit khi client không khai báo surrounding text hoặc là terminal.

## Việc cần người thử
1. `scripts/linux/build-deb.sh` rồi `sudo apt install ./dist/pkey-ibus_0.1.0_amd64.deb`, `ibus restart`.
2. Settings › Keyboard › Input Sources › + › Vietnamese › pKey.
3. Gõ thử trong gedit/GNOME Text Editor, Firefox, Chromium, VS Code, Terminal, và ô mật khẩu; ghi lại app nào gạch chân preedit sai hoặc mất ký tự.
4. Thử trên Ubuntu 20.04/22.04 (X11) nếu có.

## Nhật ký lỗi gặp khi làm
- `IBusProperty` thiếu trường cuối `symbol` (IBusText) ⇒ daemon `GLib-ERROR: Attempt to access item 11 in a container with only 11 items` và sập. Cấu trúc đúng lấy từ serialize của libibus.
- Mảng `av` bọc biến thể hai lần (`[<<(...)>>]`) khi tự gọi `Value::new` cho từng phần tử; zvariant đã tự bọc. Có test `array_elements_are_wrapped_once`.
