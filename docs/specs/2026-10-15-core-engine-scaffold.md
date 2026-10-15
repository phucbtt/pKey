# 2026-10-15 — Khung workspace và lõi engine (P0-02, P1 một phần)

**Trạng thái: code đã viết, CHƯA biên dịch và CHƯA chạy test.** Máy không có MSVC linker (xem "How to test"). Mọi hành vi bên dưới là thiết kế, chưa được kiểm chứng.

## Context / Why
Bắt đầu triển khai theo `SPEC_VietIME_Full_Build.md` và `IMPLEMENTATION_PHASES.md`: dựng workspace (P0-02) và lõi engine (P1: CORE-001/002/003/006, Telex + VNI).

## Decision
- Làm phần không phụ thuộc OS trước; các POC native (TSF/IMK/IBus/Fcitx5) cần máy thật và chưa làm.
- Giản lược so với spec (ghi lại để bù sau):
  - Telex/VNI nằm trong `ime-core::composition`, chưa tách thành `ime-telex`/`ime-vni`/`ime-viqr`.
  - Bảng vần là mảng Rust (`ime-vietnamese/src/rhymes.rs`), chưa phải `rhymes.toml`.
  - NFC render bằng `compose_char` (ghép combining rồi NFC), chưa dùng bảng tra.
  - `EngineConfig` là struct trong `ime-core`; `ime-config` (TOML, migrate) chưa có.
  - VIQR: enum có, luật chưa làm (CORE-013, P4). Macro, hotkey, profile, C ABI (`ime-ffi`) chưa làm.
  - Golden fixture dự định dùng TOML thay YAML để tránh phụ thuộc `serde_yaml`.

## Design
- `crates/ime-unicode`: `Shape`, `Tone`, `compose_char`, NFC/NFD, `Caret` (4 đơn vị).
- `crates/ime-vietnamese`: `Letter`, `analyze` (onset/nucleus/coda, xử lý `qu`/`gi`), `validity` (Valid/Prefix/Invalid), `tone_index` (§8.3), `render`.
- `crates/ime-core`: `Engine` (contexts, `process`, `would_consume`, `notify`, `set_mode`), `Composition` (áp dấu, hoàn tác khi lặp phím, backspace 2 chế độ, auto-restore), kiểu dữ liệu §7.1. `KeyInput`/`EngineAction` có `Debug` đã che nội dung gõ.
- `crates/ime-testing`, `apps/ime-cli`: mới là khung rỗng (placeholder), chưa có FakeHost, golden runner, CLI.

## API / Schema changes
Mới hoàn toàn, chưa có API công khai ổn định. Không có breaking change.

## How to test
Chưa chạy được. Cần cài C toolchain trước. Sau đó, chạy trong PowerShell (không dùng Git Bash vì `link.exe` của coreutils đè linker MSVC):
```
cargo test -p ime-unicode -p ime-vietnamese -p ime-core
```
Các test đã viết: bảng đặt dấu §8.3, valid/invalid/prefix syllable, 134 ký tự NFC, thứ tự NFD, caret, và test engine (consume, mode, secure input, cô lập context, `would_consume == process`).

Việc tiếp theo khi có toolchain: build + sửa lỗi biên dịch, rồi viết FakeHost, golden runner (fixture trong `tests/golden/`), CLI `simulate`, proptest/fuzz, `ime-ffi`.
