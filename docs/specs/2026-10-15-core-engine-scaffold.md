# 2026-10-15 — Lõi engine, FakeHost, golden runner, CLI (P0-02, P1 một phần)

**Trạng thái:** biên dịch sạch (`cargo check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`). Lúc viết spec này máy chưa có MSVC linker nên chưa chạy test; **đã chạy test sau đó, toàn bộ pass, golden 120/120** (xem `2026-10-15-hotkey-and-first-test-run.md`). Phần "Rủi ro đã biết khi chạy lần đầu" bên dưới không xảy ra với golden; chỉ có một test đơn vị sai dữ liệu đầu vào và đã sửa.

## Context / Why
Bắt đầu triển khai theo `SPEC_pKey_Full_Build.md` và `IMPLEMENTATION_PHASES.md`: dựng workspace (P0-02) và lõi engine (P1: CORE-001/002/003/006, Telex + VNI, FakeHost, golden, CLI simulate).

## Decision
- Làm phần không phụ thuộc OS trước; các POC native (TSF/IMK/IBus/Fcitx5) cần máy thật và chưa làm.
- Vì chưa có linker, `serde`, `toml`, `clap`, `proptest` (cần build script/proc-macro) bị loại khỏi workspace để còn kiểm tra biên dịch được. Thay bằng:
  - golden file định dạng dòng tự viết (`tests/golden/*.golden`) thay vì YAML/TOML;
  - phân tích tham số CLI tự viết;
  - test ngẫu nhiên bằng xorshift cố định seed thay vì proptest.
  Khi có toolchain, có thể đưa lại `clap`, `proptest`, `toml`.
- Giản lược khác so với spec (cần bù sau). **Cập nhật:** VIQR, `ime-config`, macro, hotkey, C ABI, proptest và bench đã có — xem `2026-10-15-p1-core-completion.md`; các mục còn thiếu thực sự được liệt kê ở đó. Danh sách dưới đây giữ nguyên làm lịch sử:
  - Telex/VNI nằm trong `ime-core::composition`, chưa tách `ime-telex`/`ime-vni`/`ime-viqr`.
  - Bảng vần là mảng Rust (`ime-vietnamese/src/rhymes.rs`), chưa phải `rhymes.toml`.
  - NFC render bằng `compose_char` (ghép combining rồi NFC), chưa dùng bảng tra.
  - `EngineConfig` là struct trong `ime-core`; `ime-config` (TOML, migrate) chưa có.
  - Chưa có: VIQR (CORE-013), macro, hotkey, profile, C ABI `ime-ffi`, fuzz, bench.

## Design
- `crates/ime-unicode`: `Shape`, `Tone`, `compose_char`, NFC/NFD, `Caret` (4 đơn vị).
- `crates/ime-vietnamese`: `Letter`, `analyze` (onset/nucleus/coda, `qu`/`gi`), `validity` (Valid/Prefix/Invalid), `tone_index` (§8.3), `render`.
- `crates/ime-core`: `Engine` (contexts, `process`, `would_consume`, `notify`, `set_mode`), `Composition` (áp dấu, hoàn tác khi lặp phím, backspace 2 chế độ, auto-restore). `KeyInput`/`EngineAction` có `Debug` che nội dung gõ.
  - Quy tắc hoàn tác phím `w`/`7`: chỉ hoàn tác khi nguyên âm cuối của cụm đang mang dấu của phím đó (nếu không, `uwow` sẽ phá chữ `ươ`).
- `crates/ime-testing`: `FakeHost` (ghi nhận vi phạm hợp đồng: `would_consume` lệch `process`, Backspace lọt xuống document khi đang soạn, Cancel không có preedit…), `parse_keys`, `golden` (parser + runner).
- `apps/ime-cli` (binary `pkey-cli`): `simulate`, `golden`.
- `tests/golden/telex.golden`, `vni.golden`: ~130 ca; giá trị mong đợi lấy từ chính tả tiếng Việt, không lấy từ output của engine.

## API / Schema changes
Mới hoàn toàn, chưa có API công khai ổn định. Không có breaking change.

## How to test
Cần C toolchain (Visual Studio Build Tools, workload C++). Chạy trong PowerShell, không dùng Git Bash (`link.exe` của coreutils đè linker MSVC):
```
cargo test --workspace
cargo run -p ime-cli -- simulate "tieengs vieejt "
cargo run -p ime-cli -- golden tests/golden/telex.golden tests/golden/vni.golden
```
Kiểm tra không cần linker (đã chạy được): `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`.

Rủi ro đã biết khi chạy lần đầu: một số ca golden có thể lệch do lỗi thật của engine hoặc do kỳ vọng sai; phải xét từng ca theo chính tả, không sửa kỳ vọng cho khớp output.
