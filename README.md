<div align="center">

<img src="docs/assets/pkey-icon.png" alt="pKey" width="128" height="128">

# pKey

[![ci](https://github.com/phucbtt/pKey/actions/workflows/ci.yml/badge.svg)](https://github.com/phucbtt/pKey/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

</div>

**pKey** là bộ gõ tiếng Việt đa nền tảng, hỗ trợ kiểu gõ Telex, VNI và VIQR. Lõi được viết bằng
Rust và dùng chung cho mọi nền tảng: Windows (app khay và TSF), Ubuntu/Debian (IBus).

- Gõ được ở mọi ứng dụng, không cần thêm bàn phím trong cài đặt hệ thống (bản app khay, giống EVKey).
- Kiểm tra chính tả, tự khôi phục từ không phải tiếng Việt (ví dụ `windows`), đặt dấu kiểu cũ/mới.
- Macro, profile theo ứng dụng, phím tắt đổi Việt/English.
- Cấu hình bằng một file `config.toml` dễ đọc, có kiểm tra lỗi.
- Ô mật khẩu được nhận biết và bộ gõ tự bỏ qua.

> Tài liệu kỹ thuật đầy đủ nằm trong [docs/](docs/). Đây là dự án đang phát triển; xem phần
> [Trạng thái](#trạng-thái) trước khi dùng cho công việc quan trọng.

## Mục lục

- [Trạng thái](#trạng-thái)
- [Cài đặt](#cài-đặt)
- [Sử dụng](#sử-dụng)
- [Cấu hình](#cấu-hình)
- [Build từ source](#build-từ-source)
- [Kiểm thử](#kiểm-thử)
- [Cấu trúc dự án](#cấu-trúc-dự-án)
- [Tài liệu](#tài-liệu)
- [Đóng góp](#đóng-góp)
- [Giấy phép](#giấy-phép)

## Trạng thái

| Thành phần | Trạng thái |
|---|---|
| Lõi gõ: Telex, VNI, VIQR, đặt dấu, kiểm tra chính tả, khôi phục | Ổn định. Kiểm thử vét cạn khoảng 19 000 âm tiết cho mỗi kiểu gõ |
| Cấu hình, macro, profile theo ứng dụng, phím tắt | Ổn định, có test |
| C ABI (`ime-ffi`) | Ổn định, có test và C harness chạy dưới ASan/UBSan |
| App khay Windows (`pkey.exe`, hook bàn phím, cửa sổ Cài đặt) | Chạy được; đã gõ thật vào ô soạn thảo Windows qua e2e. Chưa thử đầy đủ trên Chrome, Word, VS Code |
| Windows 7 SP1, 8, 8.1 | Có bản build và kiểm tra import tĩnh, **chưa chạy trên máy Windows 7/8 thật**, xem [spec](docs/specs/2026-10-16-windows-7-support.md). Vista và XP không hỗ trợ |
| Bàn phím TSF Windows, x64 và x86 | Chạy được trên TSF qua text store giả lập. **Chưa thử trong ứng dụng thật**. Tùy chọn |
| IBus Ubuntu/Debian | Chạy được: Telex/VNI/VIQR, preedit, VI/EN, ô mật khẩu. Kiểm thử tự động với `ibus-daemon` thật (15/15) trên Ubuntu 22.04 và 24.04. **Chưa thử tay trong các ứng dụng GTK/Qt/Chromium** |
| macOS (IMK), Fcitx5 | Chưa bắt đầu |
| Onboarding, installer có chữ ký, tự cập nhật | Chưa bắt đầu |

Chi tiết từng thành phần, hạn chế đã biết và việc cần người thử: xem [docs/specs/](docs/specs/).

## Cài đặt

### Windows 7 SP1, 8, 8.1, 10, 11

Một bản dùng cho mọi phiên bản, 32-bit, 64-bit và ARM64. Bản Windows 7/8 chưa được thử trên máy thật,
xem [Trạng thái](#trạng-thái). Windows Vista và XP không được hỗ trợ.

**Bản một file (portable).** Không cần cài đặt, không cần Visual C++ Redistributable. Chép đi đâu cũng
chạy được, kể cả trên USB. Bản này được build bằng script, xem [Build từ source](#build-từ-source).

- Chạy `dist\pKey.exe` (khoảng 1 MB).
- Cài đặt lưu theo người dùng tại `%APPDATA%\pKey`.
- Để tự chạy khi đăng nhập, bật mục "Khởi động cùng Windows" trong menu khay.
- Gỡ: thoát app, xóa file. Xóa thêm `%APPDATA%\pKey` nếu muốn xóa cả cài đặt.

**Bộ cài đặt.** `pKey-Setup-<version>.exe` cài theo người dùng, không cần quyền admin. Bản hiện **chưa
ký số**, nên Windows SmartScreen sẽ cảnh báo; chọn "More info" → "Run anyway" nếu bạn tin nguồn gốc file.

**Bàn phím TSF (tùy chọn).** Bàn phím "pKey" có thể thêm vào Settings › Time & language. Không chọn mặc định.
Không dùng cùng lúc với app khay: nếu bật cả hai, mỗi phím sẽ được xử lý hai lần.

### Ubuntu / Debian (IBus)

Cài bằng một lệnh:

```sh
curl -fsSL https://raw.githubusercontent.com/phucbtt/pKey/main/scripts/linux/install-release.sh | sh
```

Lệnh tải gói `.deb` của bản phát hành, kiểm tra SHA256, cài bằng `apt` (kéo theo `ibus`), thêm pKey vào
nguồn nhập và khởi động lại IBus. Các nguồn nhập khác của bạn được giữ nguyên.

- Chuyển sang pKey bằng `Super+Space` hoặc menu nguồn nhập trên thanh trên cùng.
- Gỡ: `sudo apt remove pkey-ibus`. Cấu hình ở `~/.config/pkey/config.toml` không bị xóa.
- Gói `.deb` là một binary tĩnh, không phụ thuộc glibc hay GTK, nên dùng chung cho các bản Ubuntu/Debian có IBus.

Cài từ source (cần Rust và `sudo`): `scripts/linux/install.sh`. Gỡ bản này bằng `scripts/linux/install.sh --uninstall`.

## Sử dụng

### Phím tắt

| Thao tác | Kết quả |
|---|---|
| `Ctrl+Shift` | Đổi giữa Tiếng Việt và English (mọi cửa sổ dùng chung chế độ) |
| Click icon khay (Windows) | Đổi Việt/English |
| Chuột phải icon khay (Windows) | Menu: kiểu gõ, kiểu đặt dấu, kiểm tra chính tả, macro, tạm dừng, cấu hình |
| `Backspace` | Sửa từ đang gõ |
| `Esc` (Windows app) | Trả lại phím thô |

### Ví dụ

| Gõ | Kết quả (Telex) |
|---|---|
| `tieengs vieejt ` | tiếng việt |
| `ddawngj` | đặng |
| `windows` | windows (tự khôi phục vì không phải tiếng Việt) |

Kiểu VNI: `tie61ng vie65t ` cho ra `tiếng việt`.

### Lưu ý

- **Ubuntu (IBus):** từ đang gõ hiện dạng preedit nên ứng dụng sẽ gạch chân. Tùy chọn
  `compat.linux_preedit = false` gõ thẳng không gạch chân, nhưng đây là tính năng thử nghiệm và có thể
  loạn phím khi gõ nhanh.
- **Windows app khay:** chữ được gõ thẳng vào ứng dụng rồi sửa dần bằng Backspace và Unicode. Có thể không
  hợp với một số ứng dụng (game, Remote Desktop, ô địa chỉ trình duyệt có gợi ý tự điền). Cửa sổ chạy quyền
  admin cần chạy pKey cũng bằng quyền admin.

## Cấu hình

Cấu hình nằm ở:

- Windows: `%APPDATA%\pKey\config.toml`
- Linux: `~/.config/pkey/config.toml`

Biến môi trường `PKEY_CONFIG_DIR` đổi thư mục cấu hình. Sửa file bằng tay được nạp lại tự động.

```sh
pkey-cli config default          # in cấu hình mặc định đầy đủ
pkey-cli config validate <file>  # kiểm tra lỗi cú pháp và giá trị
```

Cấu hình gồm kiểu gõ, kiểu đặt dấu, kiểm tra chính tả, macro (`macros.json`), profile theo ứng dụng,
game mode và phím tắt.

## Build từ source

Cần Rust (phiên bản được ghim trong [rust-toolchain.toml](rust-toolchain.toml)).

- **Windows:** cần Visual Studio Build Tools (workload C++). Chạy trong PowerShell, không dùng Git Bash,
  vì `link.exe` của coreutils sẽ đè linker MSVC.

```sh
cargo build --workspace
```

Build bản portable và bộ cài Windows:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\windows\build-portable.ps1   # dist\pKey.exe
powershell -ExecutionPolicy Bypass -File scripts\windows\build-installer.ps1  # cần Inno Setup 6
```

Bộ cài đặt cần Inno Setup 6 (`winget install JRSoftware.InnoSetup`) và tạo
`target\installer\pKey-Setup-<version>.exe` cùng file `.sha256`.

Hai script trên build cho Windows 7 trở lên bằng target `*-win7-windows-msvc`, mà Rust stable không
cung cấp sẵn: script tự cài một bản nightly được ghim trong
[scripts/windows/win7-toolchain.txt](scripts/windows/win7-toolchain.txt) (cần mạng lần đầu) và build thư viện
chuẩn từ source. Nếu `dist\pKey.exe` đang chạy, thêm `-OutDir <thư mục khác>` cho `build-portable.ps1`.
`cargo build` thông thường (stable) vẫn dùng được để phát triển, nhưng file tạo ra chỉ chạy trên Windows 10/11.

Build gói `.deb` và engine IBus tĩnh:

```sh
scripts/linux/build-deb.sh
cargo build --profile dist -p pkey-ibus --target x86_64-unknown-linux-musl
```

## Kiểm thử

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

# Golden files và mô phỏng gõ
cargo run -p ime-cli -- golden tests/golden/telex.golden tests/golden/vni.golden tests/golden/viqr.golden
cargo run -p ime-cli -- simulate "tieengs vieejt "
```

Kiểm thử đầu cuối:

- **IBus** (cần `ibus`, `python3-gi`, `gir1.2-ibus-1.0`): `scripts/linux/e2e-ibus.sh`. Chạy một `ibus-daemon`
  riêng, không đụng phiên IBus của bạn.
- **Windows app** (cần cờ `test-hooks`, không bật trong bản phát hành): `PKEY_RUN_E2E=1 cargo test -p pkey-app --features test-hooks`.

CI chạy trên Ubuntu, Windows và macOS, gồm test, clippy, C harness với sanitizer, kiểm tra IBus trên Ubuntu
22.04 và 24.04, và `cargo-deny`.

## Cấu trúc dự án

```
crates/ime-unicode      bảng ký tự, NFC/NFD, vị trí con trỏ
crates/ime-vietnamese   âm tiết, hợp lệ, đặt dấu
crates/ime-core         engine, composition, phím tắt (không unsafe)
crates/ime-config       config.toml: schema, validate, migrate, lưu atomic, profile
crates/ime-macro        macro: khớp, macros.json, CSV
crates/ime-runtime      engine + config + macro + profile, dùng chung cho mọi adapter
crates/ime-winkeys      bảng phím Windows dùng chung
crates/ime-ffi          C ABI (include/pkey.h)
crates/ime-testing      FakeHost, golden, test vét cạn, proptest
platform/windows-app    app khay (hook bàn phím), binary `pkey`
platform/windows-tsf    text service Windows (tùy chọn)
platform/linux-ibus     engine IBus cho Ubuntu/Debian, binary `ibus-engine-pkey`
apps/ime-cli            công cụ dòng lệnh `pkey-cli`
installer/              script Inno Setup
scripts/                build, cài đặt, kiểm thử theo nền tảng
tests/                  golden files, C harness, e2e IBus
docs/                   đặc tả, kế hoạch, spec cho từng thay đổi
```

## Tài liệu

- [Đặc tả đầy đủ](docs/SPEC_pKey_Full_Build.md) và [đặc tả đa nền tảng](docs/SPEC_pKey_Rust_CrossPlatform.md)
- [Kế hoạch theo phase](docs/IMPLEMENTATION_PHASES.md)
- [Spec từng thay đổi](docs/specs/): app khay Windows, TSF, IBus, và nhật ký quyết định

## Đóng góp

Mọi đóng góp đều được chào đón: báo lỗi, đề xuất tính năng, sửa tài liệu, hoặc gửi pull request.

1. Mở issue để thảo luận trước nếu thay đổi lớn.
2. Tạo nhánh từ `main`, viết test cho thay đổi, và đảm bảo `cargo test --workspace`, `cargo clippy` và
   `cargo fmt --check` đều đạt.
3. Với thay đổi không tầm thường (sửa lỗi có nguyên nhân khó thấy, tính năng mới, thay đổi mô hình dữ liệu
   hoặc quyền), thêm một spec ngắn trong [docs/specs/](docs/specs/).

Khi báo lỗi bộ gõ, vui lòng ghi rõ: hệ điều hành và phiên bản, ứng dụng bị lỗi, kiểu gõ, và các phím đã
gõ (nếu có thể tái hiện).

## Giấy phép

Phát hành theo [giấy phép MIT](LICENSE). Copyright (c) 2026 phucbtt.
