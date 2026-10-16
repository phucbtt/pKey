# 2026-10-16: Hỗ trợ Windows 7 SP1, 8, 8.1 (cùng 10 và 11)

## Yêu cầu
Một bản Windows chạy trên mọi phiên bản Windows "từ trước đến giờ". Bản cũ chỉ chạy trên Windows 10/11 vì Rust stable (từ 1.78) dùng API của Windows 10 trong thư viện chuẩn.

## Phạm vi đạt được
| Windows | Trạng thái |
|---|---|
| 10, 11 (x86, x64, ARM64) | Chạy được, đã kiểm trên Windows 11 |
| 7 SP1, 8, 8.1 (x86, x64) | **Build và kiểm tra tĩnh được; chưa chạy trên máy Windows 7/8 thật** (không có máy để thử) |
| Vista, XP | **Không hỗ trợ** (xem "Vì sao không XP/Vista") |

Windows 7 cần SP1 vì Inno Setup 6 (bộ cài) yêu cầu `6.1sp1`. Bản portable có thể chạy được trên Windows 7 chưa SP1 nhưng không được kiểm.

## Nguyên nhân và cách làm
1. **Thư viện chuẩn.** Dùng target `i686-win7-windows-msvc` / `x86_64-win7-windows-msvc` (tier 3 của Rust), phải build `std` từ source: `cargo +<nightly> build -Zbuild-std=std,panic_unwind --target ...`. Toolchain ghim ở `scripts/windows/win7-toolchain.txt` (`nightly-2026-08-01`): nightly 2026-10-15 lỗi biên dịch `std` cho target này (`is_console` không tìm thấy), nên không dùng "nightly mới nhất".
2. **`combase.dll`.** Crate `windows` 0.62 link `CoTaskMemFree` và `CoCreateFreeThreadedMarshaler` từ `combase.dll`, chỉ có từ Windows 8. Cả hai nằm ở `ole32.dll` trên Windows 7 (và `ole32` chuyển tiếp sang `combase` trên bản mới). `scripts/windows/win7-imports.ps1::Repair-Win7Imports` sửa tại chỗ bảng import của `pkey_tsf.dll` từ `combase.dll` thành `ole32.dll` (tên ngắn hơn nên vừa chỗ). Chỉ DLL bàn phím TSF cần việc này; `pkey.exe` và `pkey-cli.exe` không import `combase`.
3. **Kiểm tra tĩnh.** `Assert-Win7Imports` đọc bảng import PE và báo lỗi nếu có `api-ms-win-*`, `ext-ms-win-*`, `combase.dll`, `ucrtbase`, `vcruntime*`, `msvcp*`, `msvcr*`. Đã quét thêm tên hàm: không có `WaitOnAddress`, `GetSystemTimePreciseAsFileTime`, `ProcessPrng`, `SetThreadDescription` hay hàm Windows 8+ nào. Hàm cao nhất còn lại là của Vista (`GetFileInformationByHandleEx`, `QueryFullProcessImageNameW`, `DwmSetWindowAttribute`).
4. **Bộ cài.** Bản app và CLI là **một bản 32-bit** cho mọi Windows (x86, x64, ARM64), nên bộ cài chạy cả trên Windows 7 32-bit (bỏ `ArchitecturesAllowed=x64compatible`). DLL bàn phím TSF vẫn có cả x64 và x86; DLL x64 chỉ cài và đăng ký khi `Is64BitInstallMode`.

## File đã đổi
- `scripts/windows/build-portable.ps1`, `build-installer.ps1`: dùng target win7 và nightly ghim, gọi `win7-imports.ps1`. `build-portable.ps1` có thêm tham số `-OutDir` (để build khi `dist\pKey.exe` đang chạy).
- `scripts/windows/win7-toolchain.txt`, `win7-imports.ps1`: mới.
- `installer/windows/pkey.iss`: app x86 cho mọi máy, kiểm 64-bit cho DLL x64.
- `.github/workflows/ci.yml`: job `windows-7` build app + TSF cho target win7, sửa và kiểm import. Job đặt `RUSTFLAGS=-C target-feature=+crt-static` vì biến môi trường `RUSTFLAGS` thay thế `.cargo/config.toml`.
- README, spec app khay.

## Vì sao không XP/Vista
`std` của target win7 cần API của Vista (SRW lock, `GetFileInformationByHandleEx`, ...), và app dùng `dwmapi.dll`, `QueryFullProcessImageNameW` (Vista+). Hỗ trợ XP cần thêm lớp đệm bên thứ ba (kiểu YY-Thunks + VC-LTL), CRT khác và xử lý riêng `dwmapi`; không có máy XP/Vista để kiểm thì không thể cam kết chạy đúng. Chưa làm.

## Rủi ro còn mở
- **Chưa thử trên Windows 7/8 thật.** Bằng chứng hiện có: build thành công, import hợp lệ, self-test và 100+ test chạy trên Windows 11 bằng chính build win7, `pkey_tsf.dll` sau khi sửa import nạp được và export đủ hàm. Cần một máy hoặc VM Windows 7 SP1 để xác nhận hook bàn phím, tray, bộ cài, TSF.
- **Nightly ghim.** Target win7 là tier 3 và `-Zbuild-std` cần nightly; phải nâng `win7-toolchain.txt` thủ công khi cần. Khi Rust bỏ target này thì hướng này hết dùng được.
- Microsoft đã ngừng hỗ trợ Windows 7/8; chúng tôi chỉ đảm bảo build và kiểm tra tĩnh như trên, không có cam kết về lỗi riêng của các bản này.
- Sửa import của DLL TSF làm thay đổi file sau khi link (chưa ký số nên không phá chữ ký, nhưng nếu sau này ký số thì phải sửa trước khi ký).

## Kiểm thử đã chạy
- `build-portable.ps1 -OutDir target\release-assets`: build, `Assert-Win7Imports`, `--selftest` đạt (exit 0).
- `build-installer.ps1`: build TSF x64/x86, CLI, app, sửa và kiểm import, biên dịch Inno Setup thành công.
- `cargo +nightly-2026-08-01 test -p ime-core -p ime-config -p ime-runtime -p ime-macro -p ime-vietnamese -p pkey-app --target i686-win7-windows-msvc -Zbuild-std=std,panic_unwind`: toàn bộ đạt. E2E gõ phím thật (`PKEY_RUN_E2E=1`) chưa chạy cho bản win7 vì nó chiếm bàn phím.
