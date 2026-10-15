# pKey

pKey là bộ gõ tiếng Việt đa nền tảng (Telex, VNI, VIQR) với lõi viết bằng Rust. Đặc tả đầy đủ ở
[docs/SPEC_pKey_Full_Build.md](docs/SPEC_pKey_Full_Build.md); kế hoạch phân phase ở
[docs/IMPLEMENTATION_PHASES.md](docs/IMPLEMENTATION_PHASES.md); nhật ký thay đổi và quyết định ở
[docs/specs/](docs/specs/).

## Trạng thái

| Phần | Trạng thái |
|---|---|
| Lõi gõ (Telex/VNI/VIQR, đặt dấu, kiểm tra chính tả, khôi phục) | Xong, kiểm thử vét cạn ~19 000 âm tiết mỗi kiểu gõ |
| Cấu hình (`config.toml`), macro, profile theo ứng dụng, hotkey | Xong, có test |
| C ABI (`ime-ffi`) cho macOS/Fcitx5 | Xong, có test và C harness |
| **App khay kiểu EVKey** (`pkey.exe`, hook bàn phím, chạy mọi ứng dụng) | Chạy được; đã gõ thật vào ô soạn thảo Windows qua e2e; xem [spec app](docs/specs/2026-10-15-standalone-app.md). Chưa thử trên Chrome/Word/VS Code |
| **Windows (TSF)**, x64 + x86 | Chạy được trên TSF thật qua text store giả; **chưa thử trong ứng dụng thật**, xem [spec Windows](docs/specs/2026-10-15-windows-tsf.md) |
| **Linux / Ubuntu (IBus)** | Chạy được: Telex/VNI/VIQR, preedit, VI/EN, ô mật khẩu; kiểm thử tự động qua `ibus-daemon` thật (15/15); một binary tĩnh cho mọi bản Ubuntu/Debian; có `.deb`. **Chưa thử tay trong app thật**, xem [spec IBus](docs/specs/2026-10-15-linux-ibus.md) |
| macOS (IMK), Linux Fcitx5 | Chưa bắt đầu (cần máy thật để phát triển và kiểm chứng) |
| Settings UI, tray, onboarding, installer ký, updater | Chưa bắt đầu |

## Cấu trúc

```
crates/ime-unicode      bảng ký tự, NFC/NFD, vị trí con trỏ
crates/ime-vietnamese   âm tiết, hợp lệ, đặt dấu
crates/ime-core         engine, composition, hotkey (không unsafe)
crates/ime-config       config.toml: schema, validate, migrate, lưu atomic, profile
crates/ime-macro        macro: khớp, macros.json, CSV
crates/ime-runtime      engine + config + macro + profile + reload, dùng chung cho mọi adapter
crates/ime-ffi          C ABI (include/pkey.h)
crates/ime-testing      FakeHost, golden, test vét cạn, proptest
platform/windows-app    app khay (hook bàn phím), như EVKey
platform/linux-ibus     engine IBus cho Ubuntu/Debian (`ibus-engine-pkey`, thuần Rust qua zbus)
platform/windows-tsf    text service Windows (bàn phím cài trong Settings, tùy chọn)
crates/ime-winkeys      bảng phím Windows dùng chung
apps/ime-cli            công cụ dòng lệnh `pkey-cli`
tests/                  golden files, C harness
scripts/windows         cài / gỡ bản dev
scripts/linux           đóng gói .deb, cài không qua gói, test e2e với ibus-daemon
```

## Build và test

Cần Rust (xem `rust-toolchain.toml`). Trên Windows cần Visual Studio Build Tools (workload C++);
chạy trong PowerShell, không dùng Git Bash (`link.exe` của coreutils đè linker MSVC).

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p ime-cli -- simulate "tieengs vieejt "
cargo run -p ime-cli -- golden tests/golden/telex.golden tests/golden/vni.golden tests/golden/viqr.golden
cargo run -p ime-cli -- config validate path\to\config.toml
```

Windows 32-bit: `rustup target add i686-pc-windows-msvc`, rồi thêm `--target i686-pc-windows-msvc`.

## Ubuntu / Debian (IBus)

Bản gõ cho Linux là một engine IBus, hoạt động trên cả X11 lẫn Wayland và mọi bản Ubuntu có IBus
(GNOME mặc định đã dùng IBus). Engine là **một file tĩnh**, không phụ thuộc phiên bản thư viện hệ thống.

```
rustup target add x86_64-unknown-linux-musl     # một lần
scripts/linux/build-deb.sh                      # tạo dist/pkey-ibus_<ver>_amd64.deb
sudo apt install ./dist/pkey-ibus_*_amd64.deb
ibus restart                                    # hoặc đăng xuất rồi đăng nhập
```

Rồi vào Settings › Keyboard › Input Sources › + › Vietnamese › **pKey**. Đổi Việt/English bằng
`Ctrl+Shift` (chỉ báo VI/EN chỉ hiện nếu bảng điều khiển của desktop hỗ trợ property IBus). Cấu hình ở `~/.config/pkey/config.toml`. Từ đang gõ hiện dạng preedit nên desktop sẽ gạch chân; `compat.linux_preedit = false` gõ thẳng chữ để không gạch chân nhưng là thử nghiệm và có thể loạn phím khi gõ nhanh. Không dùng `.deb`:
`scripts/linux/install.sh` (và `--uninstall`). Cài đặt cần root một lần vì `ibus-daemon` chỉ đọc
`/usr/share/ibus/component`.

Kiểm thử không cần cài (không đụng phiên IBus đang chạy):

```
cargo build -p pkey-ibus
scripts/linux/e2e-ibus.sh
```

## Bản một file (portable)

```
powershell -ExecutionPolicy Bypass -File scripts\windows\build-portable.ps1
```

Tạo `dist\pKey.exe` (khoảng 1 MB): **một file duy nhất**, không cần cài, không cần Visual C++ Redistributable, chạy trên Windows 10/11 (32-bit, 64-bit và ARM64). Chép đi đâu cũng chạy (Desktop, USB); cài đặt lưu theo từng người dùng ở `%APPDATA%\pKey`. Bấm đúp để chạy, chuột phải vào icon khay để cấu hình, mục "Khởi động cùng Windows" để tự chạy khi đăng nhập. Gỡ: thoát app rồi xóa file (và `%APPDATA%\pKey` nếu muốn xóa cài đặt).

Đây là chương trình 32-bit, vì chương trình 32-bit chạy được trên cả máy 32-bit, 64-bit và ARM64, và hook bàn phím cùng `SendInput` hoạt động xuyên kiến trúc (đã test: bản 32-bit gõ đúng vào cửa sổ 64-bit). Windows 7/8 không được hỗ trợ (Rust bản hiện tại yêu cầu Windows 10).

## Dùng như EVKey

Cài `pKey-Setup-<ver>.exe` (hoặc chạy thẳng `target
elease\vietime-app.exe`): app nằm ở khay hệ thống, gõ được ở mọi ứng dụng. `Ctrl+Shift` hoặc click icon để đổi Việt/English; chuột phải để đổi kiểu gõ và tùy chọn.

## Thử bàn phím TSF trên Windows (tùy chọn)

```
powershell -ExecutionPolicy Bypass -File scripts\windows\pkey-dev.ps1 install
```

Chỉ ghi vào HKCU và `%LOCALAPPDATA%\pKey`, không cần quyền admin. Sau đó thêm bàn phím pKey trong
Settings › Time & language. Gỡ bằng `... pkey-dev.ps1 uninstall`. Chi tiết và rủi ro đã biết:
[docs/specs/2026-10-15-windows-tsf.md](docs/specs/2026-10-15-windows-tsf.md).

## Tạo bản cài đặt

```
powershell -ExecutionPolicy Bypass -File scripts\windows\build-installer.ps1
```

Cần Inno Setup 6 (`winget install JRSoftware.InnoSetup`). Kết quả: `target\installer\pKey-Setup-<version>.exe`
cùng file `.sha256`. Bản cài theo từng người dùng (không cần admin), **chưa ký số**, nên Windows SmartScreen sẽ cảnh báo.
Bản cài toàn máy (HKLM) có chữ ký là hạng mục riêng (REL-001a). Script: `installer/windows/pkey.iss`.

## Cấu hình

`%APPDATA%\pKey\config.toml` (Windows). `pkey-cli config default` in cấu hình mặc định đầy đủ.
Biến môi trường `PKEY_CONFIG_DIR` đổi thư mục cấu hình.
