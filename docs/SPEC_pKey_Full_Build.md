# pKey Rust — Đặc tả xây dựng đầy đủ (Full Build Specification)

| Thuộc tính | Giá trị |
|---|---|
| Phiên bản tài liệu | 2.0 (mở rộng từ `SPEC_pKey_Rust_CrossPlatform.md` v1.0) |
| Ngày | 2026-10-15 |
| Trạng thái | Đặc tả triển khai; **chưa có code, chưa có POC, chưa có benchmark** |
| Nền tảng đích | Windows 10 22H2+/11 (x64, x86 TIP, ARM64 sau), macOS 13+ (arm64, x86_64), Linux (IBus ≥ 1.5.26, Fcitx5 ≥ 5.0.21) |
| Ngôn ngữ lõi | Rust stable (MSRV cố định trong `rust-toolchain.toml`, đề xuất 1.80+) |
| Tài liệu gốc | [SPEC_pKey_Rust_CrossPlatform.md](SPEC_pKey_Rust_CrossPlatform.md) — mọi ràng buộc ở tài liệu gốc vẫn còn hiệu lực; tài liệu này bổ sung chi tiết |

---

## 0. Cách đọc tài liệu

### 0.1 Quy ước từ khóa
- **PHẢI (MUST)**: bắt buộc; vi phạm thì không được merge/release.
- **NÊN (SHOULD)**: mặc định phải làm; được bỏ qua nếu có lý do ghi trong ADR.
- **CÓ THỂ (MAY)**: tùy chọn.
- **[VERIFY]**: giả định kỹ thuật chưa kiểm chứng, phải được POC xác nhận trước khi coi là đúng.

### 0.2 Nguyên tắc bất biến (kế thừa từ spec gốc)
1. **Clean-room**: chỉ học *hành vi* của EVKey từ tài liệu công khai; không sao chép binary, branding, mã nguồn chưa rõ license.
2. **Một lõi Rust dùng chung**: deterministic, pure, không biết OS.
3. **Native IME adapter cho từng OS**: TSF (Windows), InputMethodKit (macOS), IBus + Fcitx5 (Linux). Hook toàn cục **không** thay thế IME.
4. **Privacy**: không ghi log, không lưu, không gửi mạng nội dung người dùng gõ.
5. **Fail-open**: lỗi adapter ⇒ pass-through phím và reset; không bao giờ xóa văn bản ngoài vùng composition.
6. **Không claim hỗ trợ khi chưa test** trên máy thật.

### 0.3 Cấu trúc
| Phần | Nội dung |
|---|---|
| 1–4 | Phạm vi, thuật ngữ, yêu cầu chức năng & phi chức năng |
| 5–6 | Kiến trúc, process model, Rust workspace |
| 7–12 | Lõi engine: kiểu dữ liệu, state machine, ngữ âm tiếng Việt, kiểu gõ, Unicode, macro, profile |
| 13–15 | C ABI, adapter từng OS, đồng bộ trạng thái/IPC |
| 16–18 | Cấu hình, UI/UX, CLI |
| 19–21 | Bảo mật, logging, hiệu năng |
| 22–25 | Kiểm thử, CI/CD, đóng gói, cập nhật |
| 26–29 | Roadmap, backlog, rủi ro, ADR/câu hỏi mở, checklist release |
| Phụ lục | Bảng ngữ âm, golden corpus, mapping phím, tài liệu tham khảo |

---

## 1. Phạm vi sản phẩm

### 1.1 Mục tiêu
pKey là bộ gõ tiếng Việt offline-first, đa nền tảng, cung cấp:
- Kiểu gõ Telex, VNI, VIQR; đầu ra Unicode NFC (mặc định) / NFD; v1.1 thêm TCVN3 (ABC), VNI Windows.
- Đặt dấu kiểu mới/cũ, bỏ dấu tự do, kiểm tra chính tả, tự khôi phục từ sai.
- Chuyển VI/EN bằng phím tắt, macro (gõ tắt), profile theo ứng dụng, game mode.
- Settings UI, tray/menu, onboarding, công cụ chẩn đoán (doctor) không lộ nội dung gõ.

### 1.2 Người dùng mục tiêu
| Persona | Nhu cầu chính |
|---|---|
| Người dùng văn phòng | Gõ ổn định trong Office, trình duyệt, chat; không mất ký tự |
| Lập trình viên | Không bị biến đổi khi gõ code/URL; profile tắt tiếng Việt cho IDE/terminal |
| Người dùng Linux | Hoạt động trên Wayland qua IBus/Fcitx5, không cần root |
| Game thủ | Game mode pass-through, không đụng anti-cheat |

### 1.3 Ngoài phạm vi v1 (nhắc lại)
AI/cloud autocorrect; tài khoản/sync; keylogger; gõ ở màn hình đăng nhập; bypass UAC/anti-cheat/sandbox; cam kết 100% game và ứng dụng tự vẽ text control; `exec`/script trong macro; hỗ trợ X11 global hook trên Linux; ứng dụng mobile.

---

## 2. Thuật ngữ

| Thuật ngữ | Định nghĩa |
|---|---|
| **Input context (IC)** | Một ô nhập liệu đang có focus do OS/framework cung cấp (TSF `ITfContext`, IMK client, IBus/Fcitx `InputContext`) |
| **Composition / preedit** | Văn bản tạm đang được soạn (thường gạch chân), chưa commit vào document |
| **Commit** | Ghi văn bản cuối cùng vào document và kết thúc composition |
| **Raw key history** | Chuỗi phím gốc người dùng đã nhấn cho từ hiện tại (vd. `t i e e n g s`) |
| **Syllable / âm tiết** | `onset + nucleus + coda + tone` (phụ âm đầu + vần chính + phụ âm cuối + thanh) |
| **Shape mark** | Dấu biến đổi chữ cái: mũ (â ê ô), móc (ơ ư), trăng (ă), gạch (đ) |
| **Tone mark** | Dấu thanh: sắc, huyền, hỏi, ngã, nặng |
| **Delimiter** | Phím kết thúc từ: space, Enter, Tab, dấu câu, ký tự không thuộc kiểu gõ |
| **Pass-through** | Engine không xử lý, trả phím lại cho ứng dụng |
| **Capability** | Khả năng mà host/IC hỗ trợ (preedit, surrounding text, password…) |
| **TIP** | Text Input Processor — text service trên TSF |
| **Adapter** | Lớp mã OS-specific nối native IME API với lõi Rust |

---

## 3. Yêu cầu chức năng chi tiết

Mỗi feature có ID, ưu tiên, version và **tiêu chí chấp nhận (AC)** có thể kiểm thử.

### F001 — Kiểu gõ Telex & VNI (P0, MVP)
- AC1: 100% golden fixtures Telex/VNI trong `tests/golden/` pass trên CI cả 3 OS.
- AC2: Gõ thật "Tiếng Việt có dấu đầy đủ" trong 1 ứng dụng tham chiếu mỗi OS qua native adapter.
- AC3: Lặp phím biến đổi hoàn tác (`aaa`→`aa`, `ass`→`as`, `a11`→`a1`) theo §9.
- AC4: Hoa/thường giữ đúng (`DDAWNG`→`ĐĂNG`, `Ddawng`→`Đăng`).

### F002 — Unicode NFC/NFD (P0, MVP)
- AC1: Đầu ra NFC là `unicode_normalization::is_nfc` = true; NFD ngược lại.
- AC2: Offset caret báo cho adapter đúng đơn vị của API (UTF-16 trên Windows/macOS, Unicode scalar trên IBus, byte/char theo Fcitx `Text`).
- AC3: Property test: normalize(normalize(x)) = normalize(x).

### F003 — Chuyển VI/EN (P0, MVP)
- AC1: Hotkey mặc định (§16.3) đổi mode trong < 1 phím; trạng thái hiển thị đồng bộ trên tray/menu/indicator ≤ 200 ms.
- AC2: Chế độ EN: mọi phím pass-through, engine không giữ composition.
- AC3: Đổi mode khi đang composing ⇒ commit composition hiện tại trước.
- AC4: `mode_scope = global | per_app` (§12).

### F004 — Native IME (P0, MVP → 1.0)
- AC1: Windows: TIP đăng ký qua `ITfInputProcessorProfileMgr`, xuất hiện trong danh sách bàn phím Tiếng Việt, gõ được trong Notepad.
- AC2: macOS: Input Source xuất hiện trong System Settings › Keyboard › Input Sources, gõ được trong TextEdit.
- AC3: Linux: engine IBus xuất hiện trong `ibus list-engine`; addon Fcitx5 xuất hiện trong `fcitx5-configtool`; gõ được trong gedit/Kate.

### F005 — Composition lifecycle (P0, MVP)
- AC1: Preedit hiển thị gạch chân (nếu host hỗ trợ) và commit đúng khi gặp delimiter.
- AC2: Backspace, Esc, mũi tên, click chuột, đổi focus, đổi cửa sổ không gây mất/lặp ký tự (kịch bản E2E §22.5).
- AC3: Host hủy composition ⇒ engine bỏ state, không gửi xóa.

### F006 — Tray/menu (P0, 1.0)
- Mục: VI/EN, Bật/Tắt, Telex/VNI/VIQR, Bảng mã, Profile hiện tại (read-only), Cài đặt…, Tạm dừng 15 phút/đến khi bật lại, Khởi động cùng máy, Thoát.
- AC: Mỗi mục thay đổi config/state và phản ánh ngay ở engine (≤ 500 ms).

### F007 — Tùy chọn gõ (P1, 1.0)
Đặt dấu kiểu mới/cũ, bỏ dấu tự do, kiểm tra chính tả, tự khôi phục từ sai, phím khôi phục (Esc), backspace mode, `w` đứng riêng → `ư`, `[`/`]` → `ơ`/`ư`.
- AC: Mỗi tùy chọn có golden tests riêng bật/tắt.

### F008 — VIQR (P1, 1.0)
- AC: golden VIQR pass; escape `\` cho ký tự literal; không biến đổi `?`/`.` khi không ở trong âm tiết hợp lệ.

### F009 — Macro (P1, 1.0)
- AC: CRUD, tìm kiếm, import/export JSON & CSV UTF-8, scope (global/app), chống trùng trigger, giới hạn 4096 ký tự đầu ra, vô hiệu trong secure field.

### F010 — Profile theo ứng dụng (P1, 1.0)
- AC: Rule match theo app identity của từng OS (§12.2), precedence đúng, game mode pass-through.

### F011 — Startup & onboarding (P1, 1.0)
- AC: Wizard phát hiện IME đã đăng ký/chọn hay chưa; UI phân biệt "Process đang chạy" và "IME đã được kích hoạt"; uninstaller xóa sạch đăng ký, giữ/hỏi config người dùng.

### F012 — Diagnostics/Doctor (P1, 1.0)
- AC: `pkey-cli doctor` và tab About/Doctor xuất báo cáo (OS, backend, version, capability, config đã redact); grep báo cáo không có bất kỳ chuỗi gõ thử nào.

### F013 — TCVN3 / VNI Windows (P2, 1.1)
- AC: Bảng ánh xạ có nguồn kiểm chứng (2 nguồn độc lập), test round-trip; công cụ chuyển mã clipboard yêu cầu xác nhận.

### F014 — Windows hook fallback (P2, 1.1)
- AC: Tắt mặc định; opt-in có cảnh báo; loop guard bằng `dwExtraInfo` marker; không tác động tiến trình nâng quyền (UIPI); hook callback < 1 ms p99.

### F015 — Signed updates (P2, 1.1)
- AC: Manifest ký (Ed25519/minisign), hash SHA-256, staged rollout, rollback về bản trước.

### Feature mở rộng (đề xuất, học hành vi từ tài liệu EVKey; không bắt buộc cho 1.0)
| ID | Feature | Ưu tiên | Version | AC |
|---|---|---|---|---|
| F016 | Quick Telex: `cc→ch`, `gg→gi`, `kk→kh`, `nn→ng`, `qq→qu`, `pp→ph`, `tt→th` ở đầu từ | P2 | 1.1 | Tắt mặc định; golden tests |
| F017 | Gõ tắt phụ âm cuối: `g→ng`, `h→nh`, `k→ch` khi âm tiết hợp lệ | P2 | 1.1 | Tắt mặc định |
| F018 | Tự viết hoa đầu câu | P2 | 1.1 | Chỉ khi host hỗ trợ surrounding text; tắt mặc định |
| F019 | Cho phép `f j w z` làm phụ âm đầu (tên riêng/vay mượn) | P2 | 1.0 | Ảnh hưởng spell-check |
| F020 | Sửa lại từ đã commit (re-compose) qua surrounding text | P2 | 1.2 | Chỉ khi capability `surrounding_text` + `delete_surrounding`; tắt mặc định |
| F021 | Công cụ chuyển mã văn bản/clipboard (Unicode ↔ TCVN3/VNI, bỏ dấu) | P2 | 1.1 | Thao tác chủ động, có preview |

---

## 4. Yêu cầu phi chức năng (NFR)

| ID | Yêu cầu | Mục tiêu | Cách kiểm |
|---|---|---|---|
| NFR-01 | Độ trễ lõi per-key p95 | < 1 ms (release) | Criterion `bench_process_key` |
| NFR-02 | Độ trễ end-to-end p95 | < 10 ms | Trace adapter (timestamp vào/ra, không chứa text) |
| NFR-03 | CPU idle | < 0.5% | Profiler OS, 5 phút |
| NFR-04 | RAM engine/headless | < 30 MB RSS; TSF DLL tăng < 5 MB / host process | Task Manager/`ps`/`smem` |
| NFR-05 | Khởi động | < 1 s sau logon | tracing timestamp |
| NFR-06 | Gõ nhanh | 200 WPM không rơi/lặp ký tự | Stress harness §22.6 |
| NFR-07 | Ổn định | 0 crash host app trong 8h soak test mỗi OS | Soak test |
| NFR-08 | Không panic qua FFI | 100% entrypoint bọc `catch_unwind` | Test + review |
| NFR-09 | Không truy cập mạng từ engine | 0 socket mở | Kiểm tra bằng `netstat`/Little Snitch/`ss` |
| NFR-10 | Đa ngôn ngữ UI | vi, en | Snapshot test |
| NFR-11 | A11y | Toàn bộ Settings dùng được bằng bàn phím; screen-reader label | Checklist |
| NFR-12 | Build tái lập | Lockfile commit, toolchain pinned | CI |

Mọi số liệu hiệu năng PHẢI ghi kèm cấu hình máy/OS/CPU/backend. Không công bố "nhanh hơn EVKey" khi chưa có benchmark công bằng.

---

## 5. Kiến trúc tổng thể

### 5.1 Phân lớp

```text
┌──────────────────────────────────────────────────────────────────┐
│ Host application (Notepad, Safari, gedit, ...)                   │
└───────────────▲──────────────────────────────────────────────────┘
                │ native IME protocol (TSF / IMK / IBus / Fcitx5)
┌───────────────┴──────────────────────────────────────────────────┐
│ Platform adapter (Rust + thin C++/ObjC/C shim nếu bắt buộc)      │
│  - map native key → KeyInput                                     │
│  - map EngineAction → native composition call                    │
│  - capability detection, app identity, secure field              │
└───────────────▲──────────────────────────────────────────────────┘
                │ Rust API trực tiếp (Windows TSF, IBus Rust)
                │ hoặc C ABI `ime-ffi` (macOS shim, Fcitx5 addon)
┌───────────────┴──────────────────────────────────────────────────┐
│ Rust core: ime-core (state machine, contexts)                     │
│   ├─ ime-vietnamese (syllable model, validity, tone placement)   │
│   ├─ ime-telex / ime-vni / ime-viqr (rule tables)                │
│   ├─ ime-unicode (NFC/NFD, grapheme, offsets, legacy encodings)  │
│   ├─ ime-macro                                                    │
│   └─ ime-config (load/validate/migrate/profile resolve)          │
└──────────────────────────────────────────────────────────────────┘
          ▲ đọc config (file watch) / runtime state (state bus)
┌─────────┴────────────────────────────────────────────────────────┐
│ apps/settings (Slint), apps/tray, apps/ime-cli                    │
└──────────────────────────────────────────────────────────────────┘
```

### 5.2 Process model theo OS

| OS | Process engine chạy ở đâu | Số instance engine | Thread gọi engine |
|---|---|---|---|
| Windows (TSF) | **In-process** DLL được nạp vào *mỗi* host process có focus bàn phím | 1 `Engine` / host process, nhiều `Context` | Thread UI của host (STA) |
| Windows (hook, 1.1) | Process `pkey-hook.exe` riêng của user | 1 | Thread hook riêng có message loop |
| macOS (IMK) | Process input method `pKey.app` (do macOS khởi chạy) | 1, nhiều client | Main thread |
| Linux IBus | Process `ibus-engine-pkey` (do ibus-daemon spawn) | 1, nhiều engine object | GLib main loop |
| Linux Fcitx5 | In-process addon `.so` trong `fcitx5` | 1 | Fcitx event loop |

Hệ quả bắt buộc:
- **Windows TSF DLL chạy trong process người khác** ⇒ PHẢI nhỏ, không panic, không block, không mở socket, không spawn thread nặng, không đọc file lớn mỗi phím; hỗ trợ cả x86 (cho ứng dụng 32-bit) và x64, ARM64 sau.
- Settings UI và tray là process riêng, crash không ảnh hưởng gõ.
- Config là **nguồn sự thật**; engine đọc snapshot và reload khi file đổi (§15).

### 5.3 Luồng xử lý một phím

```text
1. OS gửi key-down cho adapter (OnTestKeyDown/OnKeyDown, handleEvent:, ProcessKeyEvent, keyEvent)
2. Adapter kiểm tra nhanh: mode EN? game mode? secure field? modifier Ctrl/Alt/Cmd?
      → nếu có: PassThrough (trừ hotkey toggle)
3. Adapter dựng KeyInput { context_id, logical_key, text, modifiers, flags }
4. Engine::process(KeyInput) → Output { consumed: bool, actions: Vec<EngineAction> }
5. Adapter thực thi actions bằng API native (trong edit session nếu TSF)
6. Adapter trả consumed cho OS (pfEaten / YES / TRUE / filterAndAccept)
7. Nếu bước 5 thất bại: gọi Engine::notify(HostCommitFailed) → engine reset context,
   adapter trả consumed=false nếu còn kịp, KHÔNG replay phím
```

---

## 6. Rust workspace

### 6.1 Cây thư mục

```text
pkey/
├── Cargo.toml                    # [workspace], resolver = "2", profile settings
├── rust-toolchain.toml           # channel pinned, targets
├── deny.toml                     # cargo-deny: licenses, bans, advisories
├── crates/
│   ├── ime-core/
│   ├── ime-vietnamese/
│   ├── ime-telex/
│   ├── ime-vni/
│   ├── ime-viqr/
│   ├── ime-unicode/
│   ├── ime-macro/
│   ├── ime-config/
│   ├── ime-ffi/                  # cdylib + staticlib, cbindgen → include/pkey.h
│   ├── ime-ipc/
│   └── ime-testing/
├── platform/
│   ├── windows-tsf/              # cdylib: pKey.TSF.dll (x86, x64, arm64)
│   ├── windows-hook/             # bin, feature-gated, off by default
│   ├── macos-imk/                # Xcode/SwiftPM project + Rust staticlib
│   ├── linux-ibus/               # bin ibus-engine-pkey (+ C shim)
│   └── linux-fcitx5/             # CMake C++ addon linking libpkey_ffi.a
├── apps/
│   ├── settings/                 # Slint
│   ├── tray/                     # tray-icon + muda (Windows chính; Linux SNI tùy chọn)
│   └── ime-cli/                  # bin `pkey-cli`
├── assets/
│   ├── mappings/                 # telex.toml, vni.toml, viqr.toml, tcvn3.toml, vni_win.toml
│   ├── icons/
│   ├── i18n/                     # vi.ftl, en.ftl (Fluent)
│   └── fixtures/
├── installers/{windows,macos,linux}/
├── docs/
│   ├── ARCHITECTURE.md  SECURITY.md  COMPATIBILITY_MATRIX.md  IME_RULES.md
│   ├── adr/ADR-001-core.md ... ADR-00N
│   └── specs/                    # change specs theo ngày
├── tests/{golden,integration,e2e}/
└── .github/workflows/
```

### 6.2 Trách nhiệm & phụ thuộc từng crate

| Crate | Trách nhiệm | Phụ thuộc cho phép | Cấm |
|---|---|---|---|
| `ime-unicode` | NFC/NFD, grapheme, chuyển offset (grapheme↔utf16↔utf8↔scalar), bảng legacy | `unicode-normalization`, `unicode-segmentation` | I/O, OS API |
| `ime-vietnamese` | Mô hình âm tiết, parser, validity, đặt dấu, case | `ime-unicode`, `smallvec` | I/O |
| `ime-telex`/`ime-vni`/`ime-viqr` | Bảng luật kiểu gõ → `SchemeOp` | `ime-vietnamese` | I/O |
| `ime-macro` | Lưu trữ in-memory, match, expand | `ime-unicode` | exec, network |
| `ime-config` | Struct config, (de)serialize, validate, migrate, profile resolve, atomic write | `serde`, `toml`, `serde_json`, `thiserror` | network |
| `ime-core` | `Engine`, `Context`, state machine, hotkey, mode, kết nối các crate | tất cả crate trên, `tracing` (không log text) | OS API |
| `ime-ffi` | C ABI ổn định, `catch_unwind`, ownership | `ime-core` | — |
| `ime-ipc` | Message versioned settings/tray/doctor ↔ state bus | `serde`, `interprocess` (audit) | Gửi composition text |
| `ime-testing` | Fake host, golden runner, generators, bench | `proptest`, `criterion`, `serde_yaml` | — |

Mọi crate lõi (`ime-core` trở xuống) PHẢI `#![forbid(unsafe_code)]`; `unsafe` chỉ ở `ime-ffi` và `platform/*`, mỗi block có comment `// SAFETY:`.

### 6.3 Cấu hình build

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
panic = "unwind"      # cần unwind để catch_unwind ở biên FFI
debug = "line-tables-only"
strip = "symbols"     # symbol riêng lưu cho crash symbolication nội bộ
```

---

## 7. Lõi engine (`ime-core`)

### 7.1 Kiểu dữ liệu công khai

```rust
pub type ContextId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode { Vietnamese, English }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme { Telex, Vni, Viqr }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputEncoding { UnicodeNfc, UnicodeNfd, Tcvn3, VniWindows }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToneStyle { Modern, Traditional }   // hoà/thuý vs hòa/thúy

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalKey {
    Char,           // dùng KeyInput.text
    Backspace, Delete, Enter, Tab, Escape, Space,
    Left, Right, Up, Down, Home, End, PageUp, PageDown,
    Other,          // F-keys, media... ⇒ commit + pass-through
}

bitflags::bitflags! {
    pub struct Modifiers: u16 {
        const SHIFT = 1; const CTRL = 2; const ALT = 4; const META = 8;   // META = Win/Cmd/Super
        const CAPS_LOCK = 16; const ALT_GR = 32;
    }
    pub struct KeyFlags: u8 {
        const REPEAT = 1; const SYNTHETIC = 2; const KEY_UP = 4;
    }
    pub struct Capabilities: u32 {
        const PREEDIT             = 1 << 0;
        const SELECTION_REPLACE   = 1 << 1;
        const SURROUNDING_TEXT    = 1 << 2;
        const DELETE_SURROUNDING  = 1 << 3;
        const SECURE_INPUT        = 1 << 4;   // password field
        const FOCUS_METADATA      = 1 << 5;   // có app identity
        const TERMINAL_HINT       = 1 << 6;   // input purpose terminal (nếu OS báo)
    }
}

/// Không derive Debug để tránh log nhầm text (xem §20).
pub struct KeyInput {
    pub context_id: ContextId,
    pub logical_key: LogicalKey,
    pub text: Option<char>,          // ký tự đã qua keyboard layout (không phải scancode)
    pub physical_code: u32,          // mã vật lý chuẩn hóa (HID usage) — dùng cho hotkey
    pub modifiers: Modifiers,
    pub flags: KeyFlags,
}

pub enum EngineAction {
    UpdatePreedit { text: String, caret: Caret },
    Commit(String),
    CancelComposition,               // xóa preedit, không commit
    ModeChanged(InputMode),          // để adapter cập nhật indicator
}

pub struct Caret { pub graphemes: u32, pub utf16: u32, pub utf8: u32, pub scalars: u32 }

pub struct Output {
    pub consumed: bool,              // true ⇒ adapter "nuốt" phím
    pub actions: SmallVec<[EngineAction; 2]>,
}

pub enum Notify {
    FocusIn { app: AppIdentity, caps: Capabilities },
    FocusOut,
    CaretMoved, SelectionChanged,
    CompositionTerminatedByHost,     // OS/app hủy hoặc commit composition
    HostCommitFailed,
    Reset,
}

pub enum AppIdentity {
    Unknown,
    WindowsExe { path: String, aumid: Option<String> },
    MacBundle { bundle_id: String },
    LinuxDesktop { desktop_id: Option<String>, program: Option<String> },
}
```

### 7.2 Engine API

```rust
pub struct Engine { /* config snapshot, macro store, contexts: HashMap<ContextId, Context> */ }

impl Engine {
    pub fn new(cfg: Arc<Config>) -> Self;
    pub fn reload_config(&mut self, cfg: Arc<Config>);            // commit-safe: chỉ áp dụng ở ranh giới từ
    pub fn create_context(&mut self, info: ContextInfo) -> ContextId;
    pub fn destroy_context(&mut self, id: ContextId);

    /// TSF OnTestKeyDown: KHÔNG thay đổi state, trả cùng quyết định với process().
    pub fn would_consume(&self, ev: &KeyInput) -> bool;
    pub fn process(&mut self, ev: KeyInput) -> Output;
    pub fn notify(&mut self, id: ContextId, n: Notify) -> Output;

    pub fn mode(&self, id: ContextId) -> InputMode;
    pub fn set_mode(&mut self, id: ContextId, m: InputMode) -> Output;  // commit trước khi đổi
}
```

Bất biến:
- I1: `would_consume(e) == process(e).consumed` với cùng state (property test).
- I2: Engine **không bao giờ** sinh action ảnh hưởng văn bản ngoài composition (không có action "xóa N ký tự trước" trong mode IME). Action xóa surrounding chỉ tồn tại sau feature-flag F020 và chỉ khi capability `DELETE_SURROUNDING`.
- I3: `process` không cấp phát quá O(độ dài từ); độ dài composition tối đa `MAX_COMPOSITION = 32` grapheme — vượt quá ⇒ commit và bắt đầu từ mới.
- I4: Không panic với mọi input (fuzz).

### 7.3 Trạng thái composition

```rust
struct Context {
    state: State,                     // Idle | Composing
    mode_override: Option<InputMode>,
    profile: ResolvedProfile,         // §12
    caps: Capabilities,
    comp: Composition,
    last_word_raw: Option<RawHistory>,// cho F020, mặc định None
}

struct Composition {
    raw: SmallVec<[RawKey; 16]>,      // phím gốc, có case, dùng để restore
    letters: SmallVec<[Letter; 16]>,  // chữ cái đã biến đổi (base, shape, uppercase)
    tone: Tone,                       // None|Sac|Huyen|Hoi|Nga|Nang — thuộc âm tiết, không thuộc chữ
    literal: bool,                    // true sau khi undo bằng lặp phím ⇒ không biến đổi tiếp
    restore_dirty: bool,              // true sau backspace grapheme ⇒ không còn restore đúng raw
}

struct Letter { base: char /* a..z */, shape: Shape, upper: bool }
enum Shape { None, Circumflex, Breve, Horn, Stroke /* đ */ }
```

**Nguyên tắc:** chuỗi hiển thị luôn được **tính lại** từ `letters + tone + tone_style + encoding`; không bao giờ suy ngược phím gốc từ chuỗi hiển thị.

### 7.4 State machine

| Trạng thái | Sự kiện | Điều kiện | Hành động | Trạng thái mới |
|---|---|---|---|---|
| Idle | Phím chữ (a–z, hoặc phím của scheme khi có thể bắt đầu từ) | mode=VI, không secure, không Ctrl/Alt/Meta | append raw → recompute → `UpdatePreedit` | Composing |
| Idle | Phím khác | — | PassThrough | Idle |
| Composing | Phím chữ thường | — | append letter → `UpdatePreedit` | Composing |
| Composing | Phím shape/tone của scheme | áp dụng hợp lệ (§9) | biến đổi → `UpdatePreedit` | Composing |
| Composing | Phím shape/tone | lặp lại (undo) | bỏ biến đổi + append literal, `literal=true` | Composing |
| Composing | Phím shape/tone | không áp dụng được | append như chữ thường (nếu là chữ) hoặc commit + pass-through (nếu không phải chữ) | Composing / Idle |
| Composing | Backspace | còn > 1 grapheme | xóa theo `backspace_mode` → `UpdatePreedit` | Composing |
| Composing | Backspace | còn 1 grapheme | `CancelComposition`, consumed | Idle |
| Composing | Delimiter (space, Enter, Tab, dấu câu, số khi Telex) | — | macro check → spell restore check → `Commit(word)`; phím delimiter **pass-through** (consumed=false) | Idle |
| Composing | Esc | `restore_key=esc` | `Commit(raw literal)` (hoặc `UpdatePreedit(raw)` tùy setting), consumed | Idle |
| Composing | Mũi tên/Home/End/Delete/PageUp/Down/Other | — | `Commit(current)`, pass-through | Idle |
| Composing | Ctrl/Alt/Meta + phím | không phải hotkey | `Commit(current)`, pass-through | Idle |
| Composing | Hotkey toggle mode | — | `Commit(current)`, đổi mode, `ModeChanged`, consumed | Idle |
| Bất kỳ | FocusOut / CaretMoved / SelectionChanged | — | `Commit(current)` nếu `commit_on_focus_loss`, ngược lại `CancelComposition`; reset | Idle |
| Bất kỳ | CompositionTerminatedByHost | — | xóa state cục bộ, **không** action | Idle |
| Bất kỳ | HostCommitFailed | — | reset, log counter (không text) | Idle |
| Bất kỳ | KEY_UP | — | PassThrough (chỉ dùng cho modifier-only hotkey) | không đổi |
| Bất kỳ | `SYNTHETIC` flag | — | PassThrough | không đổi |

Delimiter commit order: engine trả `[Commit(word)]` với `consumed=false` để host tự chèn delimiter sau commit. Adapter PHẢI thực hiện commit **trước** khi trả phím cho host (TSF: edit session sync; IMK: `insertText` trước khi return NO; IBus: `commit_text` trước khi return FALSE — IBus đảm bảo thứ tự [VERIFY]; Fcitx5: `commitString` trước khi không filter).

### 7.5 Backspace
- `backspace_mode = grapheme` (mặc định): xóa `Letter` cuối; nếu tone còn chỗ đặt hợp lệ thì giữ và tính lại vị trí; nếu không còn nguyên âm thì bỏ tone. Đặt `restore_dirty = true`. Ví dụ `tiếng` ⌫ → `tiến`; `hoá` ⌫ → `hó` (tone giữ và đặt lại lên `o` vì vẫn còn nucleus); `á` ⌫ → composition rỗng ⇒ `CancelComposition`.
- `backspace_mode = raw_key`: pop raw key cuối, replay toàn bộ raw từ đầu (từ tối đa 32 phím ⇒ chi phí nhỏ). `tieengs` ⌫ → `tiêng`.

### 7.6 Tự khôi phục từ sai (auto restore)
Khi commit (delimiter) và `spell_check = true` và `auto_restore_invalid = true` và `restore_dirty = false`:
- Nếu âm tiết không hợp lệ (§8.4) **và** có ít nhất một biến đổi đã áp dụng ⇒ commit chuỗi raw literal.
- Ví dụ: `windows` (Telex) → không biến thành `ưindơs`; commit `windows`. `ddax` → `đã` hợp lệ, giữ.

### 7.7 Đa từ & ký tự không thuộc tiếng Việt
- Chữ số trong Telex là delimiter; trong VNI là phím chức năng chỉ khi đang composing và áp dụng được, ngược lại là ký tự literal (và kết thúc từ nếu không áp dụng được — giữ số trong từ như `abc123` literal).
- Ký tự không phải ASCII letter (đã qua layout, vd. `é` từ bàn phím Pháp) ⇒ commit + pass-through.
- Từ có chữ hoa giữa chừng (`iPhone`) được xử lý bình thường; spell-check sẽ restore nếu biến đổi làm hỏng.

---

## 8. Mô hình ngữ âm tiếng Việt (`ime-vietnamese`)

### 8.1 Cấu trúc âm tiết
`[onset] [medial] nucleus [coda] + tone`

**Onset hợp lệ (27 + rỗng):** `b c ch d đ g gh gi h k kh l m n ng ngh nh p ph qu r s t th tr v x` và rỗng. `p` đơn chỉ trong từ vay mượn (`pin`) — cho phép. F019 thêm `f j w z`.

Ràng buộc chính tả onset:
- `k`, `gh`, `ngh` chỉ đứng trước `i y e ê`. `c`, `g`, `ng` không đứng trước `i e ê` (trừ `gi`).
- `q` luôn đi với `u` (`qu`), `u` thuộc onset.
- `gi` + nguyên âm: `i` thuộc onset (`gia`, `giữ`). `gi` + phụ âm cuối không có nguyên âm khác ⇒ `i` là nucleus (`gì`, `gìn`, `gịt`).

**Nucleus/vần (không tính coda):**
- Đơn: `a ă â e ê i y o ô ơ u ư`
- Đôi chính: `ia iê yê ya ua uô ưa ươ`
- Có âm đệm (medial `o`/`u`): `oa oă oe uâ uê uy uơ uya uyê`
- Kèm bán nguyên âm cuối (coda `i y o u`): `ai ao au ay âu ây eo êu ia iu oi ôi ơi ui ưi ưu iêu yêu oai oay uôi ươi ươu uây oeo uyu`

**Coda hợp lệ:** `c ch m n ng nh p t` và bán nguyên âm `i y o u`.

Ràng buộc coda:
- `ch`, `nh` chỉ sau `a ê i y` (và `oa uê uy`).
- `ă`, `â` bắt buộc có coda.
- Thanh với coda tắc (`p t c ch`): chỉ `sắc` hoặc `nặng`.

Bảng đầy đủ các vần hợp lệ PHẢI nằm trong `assets/mappings/rhymes.toml` (data-driven), sinh tự động thành `phf` map lúc build; nguồn bảng ghi rõ trong `docs/IME_RULES.md`.

### 8.2 Parser
Input: `letters` (sau biến đổi shape). Output: `Syllable { onset, vowels: Range, coda, valid: Validity }`.

```text
1. Lowercase để phân tích; giữ cờ upper riêng.
2. Tách onset: thử khớp dài nhất trong [ngh, ng, nh, ch, gh, gi, kh, ph, th, tr, qu, b, c, d, đ, g, h, k, l, m, n, p, r, s, t, v, x].
   - Nếu onset="gi" và phần còn lại không có nguyên âm ⇒ onset="g", i thuộc vowels.
   - Nếu onset="qu" và phần còn lại không có nguyên âm ⇒ Incomplete.
3. vowels = dãy nguyên âm liên tiếp tiếp theo (a ă â e ê i y o ô ơ u ư).
4. coda = phần còn lại; phải ∈ coda set (hoặc prefix của coda khi đang gõ: "n" có thể thành "ng"/"nh").
5. Validity:
   - Valid:     onset+vần+coda ∈ bảng hợp lệ, ràng buộc thanh thỏa.
   - Prefix:    chưa hợp lệ nhưng có thể trở thành hợp lệ khi gõ thêm.
   - Invalid:   không thể hợp lệ.
```

### 8.3 Thuật toán đặt dấu thanh

```text
fn tone_position(syl, style) -> Option<index trong letters>:
  V = các chỉ số nguyên âm của nucleus (đã loại 'u' của qu, 'i' của gi khi là onset)
  if V rỗng: return None
  // 1. Ưu tiên nguyên âm có shape
  if V chứa "ươ" (ư theo sau ơ): return index(ơ)
  if có nguyên âm có shape (ă â ê ô ơ ư): return index nguyên âm có shape cuối cùng
  // 2. Một nguyên âm
  if |V| == 1: return V[0]
  // 3. Có phụ âm cuối (coda là phụ âm, không phải bán nguyên âm thuộc V)
  if syl.coda là phụ âm: return V[last]          // hoàn, thuyết, khoảng, huynh
  // 4. Ba nguyên âm không coda
  if |V| == 3: return V[1]                        // hoài, khuya→khuỷa, ngoáy
  // 5. Hai nguyên âm không coda
  if V ∈ {oa, oe, uy}:
      return style==Modern ? V[1] : V[0]          // hoà/hòa, khoẻ/khỏe, thuý/thúy
  return V[0]                                     // mía, múa, tài, sáo, cấy(â đã ở bước 1)
```

Ghi chú:
- `uơ` (huơ), `uâ` (quân), `uê` (huế), `oă` (xoăn), `iê/yê/uô/ươ` đều được bước 1 xử lý.
- `qu` + `y` (quý), `gi` + `a` (giá): `u`/`i` đã bị loại khỏi V ở bước parser.
- Khi đặt `tone_style=Traditional`, chỉ trường hợp `oa oe uy` không coda khác Modern.
- Test bắt buộc: `hòa/hoà`, `thúy/thuý`, `hủy/huỷ`, `khỏe/khoẻ`, `quý`, `giá`, `gì`, `người`, `khuyến`, `thuở`, `hoàng`, `tuổi`, `rượu`, `quốc`.

### 8.4 Kiểm tra chính tả (validity)
- `spell_check = true`: biến đổi shape/tone chỉ áp dụng nếu kết quả là `Valid` hoặc `Prefix`; nếu không, phím được xử lý như chữ thường (literal).
- `spell_check = false`: áp dụng biến đổi miễn là có nguyên âm mang được dấu; không restore khi commit.
- Không dùng từ điển từ (word dictionary) trong v1 — chỉ kiểm tra cấu trúc âm tiết, để không chặn tên riêng.

### 8.5 Hoa/thường
- Case của chữ sau biến đổi = case của **chữ gốc** (phím đầu): `Aa`→`Â`, `aA`→`â`, `DD`→`Đ`, `Dd`→`Đ`.
- Caps Lock: adapter đã gửi `text` theo layout (đã áp dụng Caps); engine không tự xử lý Caps.

---

## 9. Kiểu gõ (rule tables)

Luật được định nghĩa bằng data (`assets/mappings/*.toml`) và một bộ thực thi chung. Mỗi phím được tra thành một `SchemeOp`:

```rust
enum SchemeOp {
    Letter(char),
    Shape { target: ShapeTarget, shape: Shape },  // vd. Circumflex trên a|e|o
    HornSmart,                                    // Telex 'w' / VNI '7' / VIQR '+'
    Tone(Tone),
    RemoveMarks,                                  // Telex 'z', VNI '0'
    Stroke,                                       // đ
    Escape,                                       // VIQR '\'
}
```

Thuật toán áp dụng chung:
1. Tra `SchemeOp` cho phím (có thể nhiều ứng viên).
2. Với `Shape`/`HornSmart`/`Stroke`: tìm nguyên âm/`d` phù hợp — nếu `free_marking=true` thì tìm ngược trong toàn bộ âm tiết; nếu `false` thì chỉ chữ ngay trước.
3. Nếu ký tự đích đã có đúng shape đó ⇒ **undo**: bỏ shape, append phím literal, `literal=true`.
4. Thử từng ứng viên theo thứ tự ưu tiên; chọn ứng viên đầu tiên cho kết quả hợp lệ (§8.4). Không có ứng viên ⇒ phím là chữ thường (nếu là chữ cái) hoặc delimiter.
5. Với `Tone`: nếu tone hiện tại trùng ⇒ undo (bỏ tone, append literal); khác ⇒ thay tone.

### 9.1 Telex

| Phím | Op | Ví dụ |
|---|---|---|
| `a` sau `a` | Circumflex(a) | `aa`→`â` |
| `e` sau `e` | Circumflex(e) | `ee`→`ê` |
| `o` sau `o` | Circumflex(o) | `oo`→`ô` |
| `w` | HornSmart: ưu tiên `uo`→`ươ`, rồi `a`→`ă`, `o`→`ơ`, `u`→`ư` | `uow`→`ươ`, `aw`→`ă`, `ow`→`ơ`, `uw`→`ư` |
| `w` đầu từ / sau phụ âm | `ư` nếu `telex_w_standalone=true` | `w`→`ư`, `tw`→`tư` |
| `d` sau `d` | Stroke | `dd`→`đ` |
| `s f r x j` | Tone sắc/huyền/hỏi/ngã/nặng | `as`→`á` |
| `z` | RemoveMarks (bỏ tone; nếu không có tone thì bỏ shape gần nhất) | `áz`→`a` |
| `[` `]` | `ơ` `ư` khi `telex_brackets=true` | |

Ngoại lệ HornSmart: sau `qu`, `w` chỉ áp lên `o` (`quow`→`quơ`, không phải `qươ`). `uo` + coda / không coda đều → `ươ` (`huow`→`hươ`, `nguwowif`→`người`, `nguowif`→`người`).

Undo bằng lặp: `aaa`→`aa`, `aww`→`aw`, `ddd`→`dd`, `ass`→`as`, `ww`→`w` (khi standalone).

### 9.2 VNI

| Phím | Op |
|---|---|
| `1` `2` `3` `4` `5` | sắc, huyền, hỏi, ngã, nặng |
| `0` | RemoveMarks |
| `6` | Circumflex lên `a e o` (ưu tiên nguyên âm gần nhất hợp lệ) |
| `7` | HornSmart lên `o u` (`uo7`→`ươ`) |
| `8` | Breve lên `a` |
| `9` | Stroke `d` |

Số chỉ là phím chức năng khi đang Composing và áp dụng được; nếu không ⇒ literal digit (giữ trong composition nếu từ có chữ, ví dụ `mp3`, rồi spell-check restore khi commit). Lặp số ⇒ undo (`a11`→`a1`).

### 9.3 VIQR

| Phím | Op |
|---|---|
| `'` `` ` `` `?` `~` `.` | sắc, huyền, hỏi, ngã, nặng |
| `^` | Circumflex (a e o) |
| `(` | Breve (a) |
| `+` | Horn (o u, `uo+`→`ươ`) |
| `d` sau `d`, hoặc `-` sau `d` | Stroke |
| `\` | Escape: phím tiếp theo literal |

Quy tắc phân biệt dấu câu: ký tự tone VIQR chỉ được dùng làm dấu khi đang Composing **và** âm tiết hiện tại `Valid` sau khi áp; ngược lại là delimiter (commit + pass-through). Người dùng gõ `\?` để ép literal. Vì `.`/`?` cuối câu sau một từ hợp lệ sẽ bị hiểu là dấu, mặc định PHẢI có thêm luật: lặp tone key (`..`) ⇒ undo + literal. Ghi rõ hạn chế này trong UI.

### 9.4 Định dạng file mapping (ví dụ `telex.toml`)

```toml
schema_version = 1
scheme = "telex"

[keys]
s = { op = "tone", tone = "sac" }
f = { op = "tone", tone = "huyen" }
r = { op = "tone", tone = "hoi" }
x = { op = "tone", tone = "nga" }
j = { op = "tone", tone = "nang" }
z = { op = "remove_marks" }
w = { op = "horn_smart", standalone = "u_horn" }
a = { op = "double", target = "a", shape = "circumflex" }
e = { op = "double", target = "e", shape = "circumflex" }
o = { op = "double", target = "o", shape = "circumflex" }
d = { op = "double", target = "d", shape = "stroke" }

[options.brackets]
"[" = { op = "literal_vowel", base = "o", shape = "horn" }
"]" = { op = "literal_vowel", base = "u", shape = "horn" }
```

Mapping được nhúng vào binary lúc build (`include_str!` + parse/validate trong build script); v1 **không** cho người dùng nạp mapping tùy ý (giảm bề mặt lỗi). Kiểu gõ tùy biến là backlog sau 1.1.

---

## 10. Unicode & bảng mã (`ime-unicode`)

- Biểu diễn nội bộ: `Letter` + `Tone`; render ra NFC bằng bảng precomposed (134 ký tự tiếng Việt × hoa/thường) — không gọi normalize mỗi phím; NFD = base + combining (U+0300 huyền, U+0301 sắc, U+0303 ngã, U+0309 hỏi, U+0323 nặng, U+0302 mũ, U+0306 trăng, U+031B móc; `đ` = U+0111 không tách).
- Thứ tự combining trong NFD PHẢI đúng canonical ordering (test đối chiếu `unicode-normalization`).
- `Caret` tính đủ 4 đơn vị; adapter chọn đơn vị của mình.
- Legacy (v1.1): `Tcvn3`, `VniWindows` là **bảng ánh xạ byte/ký tự cho font cũ**, chỉ dùng khi commit; preedit vẫn hiển thị Unicode nếu host không hiển thị được — [VERIFY] hành vi mong muốn với người dùng font cũ. Bảng PHẢI có ≥ 2 nguồn đối chiếu, test round-trip cho toàn bộ 134 ký tự.

---

## 11. Macro (`ime-macro`)

### 11.1 Schema (`macros.json`)

```json
{
  "schema_version": 1,
  "macros": [
    {
      "id": "8f0c…",
      "trigger": "vn",
      "expansion": "Việt Nam",
      "case_sensitive": false,
      "whole_word": true,
      "enabled": true,
      "scope": { "type": "global" },
      "priority": 0,
      "case_follow": true
    }
  ]
}
```
`scope`: `{"type":"global"}` | `{"type":"profile","profile_id":"…"}`.

### 11.2 Hành vi
- Kích hoạt khi commit tại delimiter thuộc `expand_on`; so khớp với **raw literal** và với **chuỗi hiển thị** (cả hai đều cho phép, ưu tiên raw), để `vn` và `vn` sau biến đổi đều ra macro.
- `case_follow`: `VN`→`VIỆT NAM`, `Vn`→`Việt Nam`.
- Trùng trigger: cùng scope ⇒ từ chối khi lưu; khác scope ⇒ profile > global, rồi `priority` cao hơn.
- Giới hạn: trigger ≤ 32 ký tự, expansion ≤ 4096 ký tự, ≤ 10 000 macro; vượt ⇒ lỗi validate.
- Tắt trong: secure field, game mode, profile có `macros_enabled=false`.
- Expansion chứa xuống dòng: commit nguyên chuỗi; nếu host không nhận `\n` qua commit [VERIFY từng OS] ⇒ ghi hạn chế.
- Import CSV: cột `trigger,expansion[,case_sensitive,whole_word,enabled]`, UTF-8 (có/không BOM), báo cáo dòng lỗi; import luôn có preview + backup trước khi ghi.
- Lưu trữ: `macros.json` cạnh config; nội dung macro là dữ liệu người dùng ⇒ không bao giờ đưa vào log/diagnostics (chỉ đếm số lượng).

---

## 12. Profile theo ứng dụng & game mode

### 12.1 Schema
Xem §16.2 `[[profiles]]`.

### 12.2 App identity theo OS

| OS | Nguồn | Giá trị match |
|---|---|---|
| Windows | `GetModuleFileNameW(NULL)` trong TIP (in-process); `GetCurrentApplicationUserModelId` cho app packaged | `exe_name` (vd. `code.exe`), `process_path`, `aumid` |
| macOS | `[client bundleIdentifier]` của IMK client | `bundle_id` |
| Linux IBus | `FocusInId(object_path, client)` (IBus ≥ 1.5.27 [VERIFY]) → client như `gtk3-im:firefox` | `ibus_client`, `program` |
| Linux Fcitx5 | `InputContext::program()`, `frontendName()` | `program` |

### 12.3 Thuật toán resolve
```text
effective = global_defaults
for profile in profiles sorted by (specificity desc, order asc):   # exact > glob
    if profile.enabled && matches(profile.match, app_identity):
        effective = effective.overlay(profile.override); break   # chỉ 1 profile thắng
if caps.SECURE_INPUT: effective.mode = English; macros off      # security luôn thắng
if effective.game_mode: effective.mode = English (pass-through hoàn toàn)
```
Thứ tự ưu tiên: **security/capability > profile khớp > global**. Không có identity ⇒ global.

### 12.4 `mode_scope`
- `global` (mặc định): một trạng thái VI/EN cho cả hệ thống (§15).
- `per_app`: nhớ mode theo app identity trong runtime state (không ghi raw text, chỉ `app_key → mode`), lưu vào `state.json` để giữ qua phiên.

### 12.5 Game mode
Pass-through mọi phím, không tạo composition, không macro. Không chạm đến tiến trình game ngoài cơ chế IME chuẩn. Không cam kết game dùng raw input.

---

## 13. C ABI (`ime-ffi`)

Dùng cho macOS shim và Fcitx5 addon (và tùy chọn IBus C shim). Windows TSF và IBus Rust gọi `ime-core` trực tiếp.

### 13.1 Header (sinh bằng cbindgen, `include/pkey.h`)

```c
#define VT_ABI_VERSION 1

typedef struct VtEngine VtEngine;
typedef struct VtResult VtResult;
typedef uint64_t VtContextId;

typedef enum VtStatus {
  VT_OK = 0, VT_ERR_NULL = 1, VT_ERR_INVALID_UTF8 = 2, VT_ERR_CONFIG = 3,
  VT_ERR_UNKNOWN_CONTEXT = 4, VT_ERR_ABI_MISMATCH = 5, VT_ERR_PANIC = 255
} VtStatus;

typedef enum VtKey {
  VT_KEY_CHAR = 0, VT_KEY_BACKSPACE, VT_KEY_DELETE, VT_KEY_ENTER, VT_KEY_TAB,
  VT_KEY_ESCAPE, VT_KEY_SPACE, VT_KEY_LEFT, VT_KEY_RIGHT, VT_KEY_UP, VT_KEY_DOWN,
  VT_KEY_HOME, VT_KEY_END, VT_KEY_PAGE_UP, VT_KEY_PAGE_DOWN, VT_KEY_OTHER
} VtKey;

typedef struct VtKeyEvent {
  uint32_t struct_size;      /* = sizeof(VtKeyEvent), cho phép mở rộng */
  VtKey    key;
  uint32_t ch;               /* Unicode scalar, 0 nếu không có */
  uint32_t physical_code;    /* HID usage */
  uint16_t modifiers;        /* VT_MOD_* */
  uint8_t  flags;            /* VT_FLAG_REPEAT | VT_FLAG_SYNTHETIC | VT_FLAG_KEY_UP */
} VtKeyEvent;

typedef enum VtActionKind {
  VT_ACTION_UPDATE_PREEDIT = 1, VT_ACTION_COMMIT = 2,
  VT_ACTION_CANCEL = 3, VT_ACTION_MODE_CHANGED = 4
} VtActionKind;

typedef struct VtActionView {
  VtActionKind kind;
  const uint8_t *text_utf8;  /* mượn từ VtResult, KHÔNG null-terminated đảm bảo; dùng text_len */
  size_t   text_len;
  uint32_t caret_utf16, caret_scalars, caret_graphemes;
  uint32_t mode;             /* cho MODE_CHANGED: 0=VI, 1=EN */
} VtActionView;

typedef struct VtContextInfo {
  uint32_t struct_size;
  const uint8_t *app_id_utf8; size_t app_id_len;  /* có thể NULL */
  uint32_t app_id_kind;      /* 0 unknown, 1 win exe, 2 mac bundle, 3 linux program */
  uint32_t capabilities;     /* VT_CAP_* */
} VtContextInfo;

uint32_t   vt_abi_version(void);
VtStatus   vt_engine_new(const uint8_t *config_toml, size_t len, VtEngine **out);
VtStatus   vt_engine_new_from_default_path(VtEngine **out);
void       vt_engine_free(VtEngine *e);
VtStatus   vt_engine_reload_config(VtEngine *e, const uint8_t *toml, size_t len);
VtStatus   vt_context_create(VtEngine *e, const VtContextInfo *info, VtContextId *out);
void       vt_context_destroy(VtEngine *e, VtContextId id);
VtStatus   vt_context_update(VtEngine *e, VtContextId id, const VtContextInfo *info); /* focus-in */
int        vt_would_consume(const VtEngine *e, VtContextId id, const VtKeyEvent *ev);
VtStatus   vt_process_key(VtEngine *e, VtContextId id, const VtKeyEvent *ev, VtResult **out);
VtStatus   vt_notify(VtEngine *e, VtContextId id, uint32_t notify_kind, VtResult **out);
VtStatus   vt_set_mode(VtEngine *e, VtContextId id, uint32_t mode, VtResult **out);
int        vt_result_consumed(const VtResult *r);
size_t     vt_result_len(const VtResult *r);
VtStatus   vt_result_get(const VtResult *r, size_t i, VtActionView *out);
void       vt_result_free(VtResult *r);
```

### 13.2 Quy tắc FFI
- Mọi hàm `extern "C"` bọc `std::panic::catch_unwind`; panic ⇒ `VT_ERR_PANIC`, engine đánh dấu poisoned, các lần gọi sau trả `consumed=0` (fail-open) cho đến khi `vt_engine_free`.
- Ownership: con trỏ do Rust cấp (`VtEngine*`, `VtResult*`) chỉ được giải phóng bằng hàm `*_free` tương ứng; con trỏ text trong `VtActionView` chỉ hợp lệ đến khi `vt_result_free`.
- Không thread-safe: mỗi `VtEngine` chỉ dùng từ một thread; vi phạm là UB được tài liệu hóa (debug build assert thread id).
- `struct_size` cho phép thêm trường cuối struct mà không phá ABI; đổi ngữ nghĩa ⇒ tăng `VT_ABI_VERSION`.
- Test ABI: C test harness (`tests/ffi/`) build bằng `cc` crate, chạy dưới ASan/UBSan trên Linux/macOS.

---

## 14. Platform adapters

### 14.1 Windows — TSF Text Service (`platform/windows-tsf`)

**Sản phẩm:** `pKey.TSF.dll` (cdylib Rust dùng crate `windows`) — build x64 **và** x86 (ứng dụng 32-bit trên Windows 64-bit nạp TIP 32-bit); ARM64 (+ ARM64X/ARM64EC [VERIFY]) sau khi có máy test.

**Đăng ký (trong installer, chạy `regsvr32` hoặc custom action gọi `DllRegisterServer`):**
- COM: `HKLM\Software\Classes\CLSID\{CLSID_pKey}\InprocServer32` = đường dẫn DLL, `ThreadingModel=Apartment`.
- `ITfInputProcessorProfileMgr::RegisterProfile(CLSID, LANGID 0x042A (vi-VN), GUID_PROFILE, description, icon, …)`.
- `ITfCategoryMgr::RegisterCategory` cho: `GUID_TFCAT_TIP_KEYBOARD`, `GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT`, `GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT`, `GUID_TFCAT_TIPCAP_UIELEMENTENABLED`, `GUID_TFCAT_TIPCAP_SECUREMODE` (chỉ nếu đã kiểm chứng an toàn) [VERIFY], `GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER`.
- Không ghi registry để ép làm bàn phím mặc định; onboarding hướng dẫn người dùng thêm qua Settings › Time & language.

**COM objects & interface:**
| Interface | Vai trò |
|---|---|
| `ITfTextInputProcessorEx` | `ActivateEx`/`Deactivate`: tạo/hủy `Engine`, đăng ký sinks |
| `ITfThreadMgrEventSink` | `OnSetFocus` (document manager) ⇒ `Notify::FocusIn/FocusOut` |
| `ITfKeyEventSink` | `OnTestKeyDown` (gọi `would_consume`), `OnKeyDown` (gọi `process`), `OnTestKeyUp/OnKeyUp`, `OnPreservedKey` |
| `ITfTextEditSink` | Phát hiện selection/caret thay đổi ngoài composition ⇒ `CaretMoved` |
| `ITfCompositionSink` | `OnCompositionTerminated` ⇒ `CompositionTerminatedByHost` |
| `ITfDisplayAttributeProvider` + `IEnumTfDisplayAttributeInfo` | Thuộc tính gạch chân cho preedit |
| `ITfLangBarItemButton` | Indicator VI/EN trên taskbar/language bar, menu nhanh |
| `ITfCompartmentEventSink` | Theo dõi `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` (global) để đồng bộ VI/EN |
| `ITfKeystrokeMgr::PreserveKey` | Đăng ký hotkey có modifier (vd. Alt+Z); modifier-only (Ctrl+Shift) xử lý trong key sink theo §16.3 |

**Thực thi action:**
- `UpdatePreedit`: trong `ITfEditSession::DoEditSession` (yêu cầu `TF_ES_READWRITE | TF_ES_SYNC`; nếu không được sync thì `TF_ES_ASYNCDONTCARE` và engine phải chịu được [VERIFY]) — nếu chưa có composition: `ITfContextComposition::StartComposition` tại selection; `ITfRange::SetText`; gán display attribute; đặt selection về caret (UTF-16).
- `Commit`: `SetText` lên range composition rồi `ITfComposition::EndComposition`; nếu không có composition (commit thẳng) dùng `InsertTextAtSelection`.
- `CancelComposition`: `SetText("")` + `EndComposition`.
- Thất bại edit session ⇒ `Notify::HostCommitFailed`.

**Secure field:** đọc `GUID_PROP_INPUTSCOPE` / `ITfInputScope::GetInputScopes` chứa `IS_PASSWORD` ⇒ caps `SECURE_INPUT`.

**AppContainer/UWP & ứng dụng low-integrity:** TIP chạy trong sandbox; DLL và file config snapshot PHẢI đọc được bởi `ALL APPLICATION PACKAGES` (SID `S-1-15-2-1`) và low integrity. Thiết kế: Settings ghi `%LOCALAPPDATA%\pKey\runtime\config.snapshot` (bản rút gọn, không chứa macro? — macro cần ⇒ đưa vào snapshot), set ACL đọc cho `S-1-15-2-1`; TIP đọc file này, nếu thất bại dùng default compiled-in. **[VERIFY trong WIN-001]**; fallback: snapshot trong `HKCU\Software\pKey\Runtime` (AppContainer có quyền đọc HKCU hạn chế [VERIFY]).

**Reload config trong TIP:** không tạo thread watcher trong mọi process. Mỗi lần `OnSetFocus`, TIP kiểm tra `generation` (8 byte đầu file snapshot hoặc named shared memory `Local\pKey-Gen`) và reload nếu đổi — chi phí O(1).

**Ràng buộc nghiêm:** không `MessageBox`, không blocking I/O lớn trong callback; không `LoadLibrary` thêm DLL ngoài hệ thống; tất cả callback COM bọc `catch_unwind` → trả `S_OK`/`E_FAIL` thích hợp và fail-open (`*pfEaten = FALSE`).

**Hạn chế đã biết (ghi vào COMPATIBILITY_MATRIX):** ứng dụng không dùng TSF/IMM32 đầy đủ (một số game, terminal cũ), ứng dụng nâng quyền vẫn nạp TIP bình thường nhưng tray/settings không điều khiển được trực tiếp, console legacy conhost [VERIFY], Windows Terminal hỗ trợ TSF.

**POC exit (WIN-001):** gõ `tieengs ` → `tiếng ` trong Notepad và Chrome; TIP x86 hoạt động trong ứng dụng 32-bit; không crash khi đổi focus nhanh 100 lần.

### 14.2 Windows — Hook fallback (`platform/windows-hook`, 1.1, opt-in)
- `SetWindowsHookExW(WH_KEYBOARD_LL)` trong process `pkey-hook.exe` (per-user, medium IL), message loop riêng; callback chỉ enqueue và quyết định nhanh, tổng < 1 ms.
- Output bằng `SendInput` với backspace + Unicode (`KEYEVENTF_UNICODE`); mọi input tổng hợp mang `dwExtraInfo = PKEY_MAGIC` để bỏ qua khi quay lại hook (loop guard).
- Chỉ bật cho app nằm trong danh sách được người dùng chọn; luôn tắt khi foreground là tiến trình nâng quyền (UIPI sẽ chặn) hoặc secure desktop.
- Cảnh báo UI rõ: chế độ này dùng backspace giả, có thể sai với autocomplete.
- Không cùng lúc với TIP trên cùng app (TIP active ⇒ hook pass-through).

### 14.3 macOS — InputMethodKit (`platform/macos-imk`)

**Sản phẩm:** `pKey.app` (bundle input method) cài vào `/Library/Input Methods/` (pkg) hoặc `~/Library/Input Methods/` (dev). Swift (hoặc ObjC) shim + `libpkey_ffi.a` (universal arm64+x86_64 bằng `lipo`).

**Info.plist (tối thiểu):**
| Key | Giá trị |
|---|---|
| `CFBundleIdentifier` | `<reverse-dns>.inputmethod.pKey` (TIS yêu cầu có `.inputmethod.` [VERIFY]) |
| `InputMethodConnectionName` | `<bundle id>_Connection` |
| `InputMethodServerControllerClass` | `pKeyInputController` |
| `InputMethodServerDelegateClass` | (tùy chọn) |
| `LSBackgroundOnly` | `YES` |
| `TISInputSourceID` | `<bundle id>` |
| `TISIntendedLanguage` | `vi` |
| `tsInputMethodCharacterRepertoireKey` | `["Latn"]` |
| `tsInputMethodIconFileKey` | icon template (pdf/tiff) |
| `ComponentInputModeDict` | mode Telex/VNI/VIQR (tùy chọn hiển thị nhiều input mode) |

**Controller (`IMKInputController` subclass):**
| Method | Hành vi |
|---|---|
| `initWithServer:delegate:client:` | `vt_context_create` với `bundleIdentifier` của client |
| `activateServer:` | `vt_context_update` (focus-in, caps), cập nhật menu |
| `deactivateServer:` | `vt_notify(FocusOut)` → commit nếu có |
| `recognizedEvents:` | `NSEventMaskKeyDown | NSEventMaskFlagsChanged` |
| `handleEvent:client:` | map `NSEvent` → `VtKeyEvent`; `vt_process_key`; thực thi actions; return consumed |
| `commitComposition:` | hệ thống yêu cầu commit ⇒ commit composition hiện tại |
| `menu` | trả `NSMenu` (VI/EN, scheme, Settings…) |

**Thực thi action:**
- `UpdatePreedit`: `[client setMarkedText: attributed(text, underline) selectionRange: NSMakeRange(caret_utf16, 0) replacementRange: NSMakeRange(NSNotFound, 0)]`.
- `Commit`: `[client insertText: text replacementRange: NSMakeRange(NSNotFound, 0)]`.
- `Cancel`: `setMarkedText:@""`.

**Secure input:** khi Secure Event Input bật, macOS không giao phím cho IME bên thứ ba thông thường — engine không cần xử lý; ghi nhận trong doctor qua `IsSecureEventInputEnabled()`.

**Config & state:** một process duy nhất ⇒ dùng `FSEvents`/`DispatchSource` watch thư mục config; VI/EN global nằm trong process. Settings app riêng (`pKey Settings.app`) ghi config; nút "Bật lại input method" gửi `NSDistributedNotificationCenter` notification `…pKey.reload` (không mang dữ liệu gõ).

**Ký & notarize:** Developer ID Application + Installer, hardened runtime, notarization, staple. Không yêu cầu Accessibility/Input Monitoring cho đường IMK.

**Hạn chế đã biết:** một số app tự xử lý marked text kém (Electron cũ, terminal), Secure Input chặn IME; ghi vào ma trận.

**POC exit (MAC-001):** input source xuất hiện và chọn được; gõ `tieengs ` trong TextEdit và Safari; đổi app qua lại không mất ký tự.

### 14.4 Linux — IBus (`platform/linux-ibus`)

**Lựa chọn ADR-004 (đề xuất):** binary Rust `ibus-engine-pkey` + **C shim nhỏ** subclass `IBusEngine` bằng libibus (GObject), gọi vào Rust qua C ABI. Phương án thay thế: thuần Rust qua `zbus` cài đặt giao diện `org.freedesktop.IBus.Engine` trên bus riêng của IBus — ít phụ thuộc C nhưng phải tự serialize `IBusText` variant; quyết định sau POC LIN-001.

**Component XML** `/usr/share/ibus/component/pkey.xml`:
```xml
<component>
  <name>org.pkey.IBus.pKey</name>
  <description>pKey Vietnamese Input Method</description>
  <exec>/usr/libexec/ibus-engine-pkey --ibus</exec>
  <version>1.0.0</version>
  <license>(license dự án)</license>
  <textdomain>pkey</textdomain>
  <engines>
    <engine>
      <name>pkey</name>
      <language>vi</language>
      <license>(license dự án)</license>
      <longname>pKey</longname>
      <description>Telex/VNI/VIQR</description>
      <icon>/usr/share/pkey/icons/pkey.svg</icon>
      <layout>default</layout>
      <rank>50</rank>
      <setup>/usr/bin/pkey-settings</setup>
    </engine>
  </engines>
</component>
```

**Mapping callback:**
| IBus | Engine |
|---|---|
| `process_key_event(keyval, keycode, state)` | keyval → `char` (`ibus_keyval_to_unicode`), `IBUS_RELEASE_MASK` → `KEY_UP`; trả consumed |
| `focus_in` / `focus_in_id` | `FocusIn` + app identity |
| `focus_out` | `FocusOut` |
| `reset` | `Reset` (commit/cancel theo setting) |
| `enable` / `disable` | khởi tạo/hủy context |
| `set_capabilities` | `IBUS_CAP_PREEDIT_TEXT`, `IBUS_CAP_SURROUNDING_TEXT` |
| `set_content_type(purpose, hints)` | `IBUS_INPUT_PURPOSE_PASSWORD/PIN` ⇒ `SECURE_INPUT`; `TERMINAL` ⇒ `TERMINAL_HINT` |
| `property_activate` | menu VI/EN, scheme |

**Action:**
- `UpdatePreedit`: `ibus_engine_update_preedit_text_with_mode(text(+underline attr), cursor=caret_scalars, visible=TRUE, IBUS_ENGINE_PREEDIT_COMMIT)` — mode COMMIT để khi focus mất preedit được commit thay vì mất.
- `Commit`: `ibus_engine_commit_text`, sau đó ẩn preedit.
- Indicator: `IBusPropList` với property `InputMode` (label "VI"/"EN").
- Không có capability PREEDIT ⇒ fallback: commit-per-key không khả thi an toàn ⇒ dùng preedit trong panel (lookup aux) [VERIFY] hoặc báo LIMITED.

**Config & state:** một process ⇒ `inotify` watch thư mục config (crate `notify`); VI/EN global trong process.

### 14.5 Linux — Fcitx5 (`platform/linux-fcitx5`)

**Sản phẩm:** `pkey.so` (C++17, CMake, `Fcitx5::Core`) link tĩnh `libpkey_ffi.a`.

**Files:**
- `/usr/lib/$(arch)/fcitx5/pkey.so`
- `/usr/share/fcitx5/addon/pkey.conf`:
  ```ini
  [Addon]
  Name=pKey
  Category=InputMethod
  Version=1.0.0
  Library=pkey
  Type=SharedLibrary
  OnDemand=True
  Configurable=True
  ```
- `/usr/share/fcitx5/inputmethod/pkey.conf`:
  ```ini
  [InputMethod]
  Name=pKey
  Icon=pkey
  Label=VI
  LangCode=vi
  Addon=pkey
  Configurable=True
  ```

**Engine (`InputMethodEngineV2`):**
| Fcitx5 | Engine |
|---|---|
| `keyEvent(entry, KeyEvent&)` | `keyEvent.key().sym()` → `Key::keySymToUnicode`; `isRelease()` → `KEY_UP`; consumed ⇒ `keyEvent.filterAndAccept()` |
| `activate` / `deactivate` | `FocusIn` / `FocusOut` |
| `reset` | `Reset` |
| Per-IC state | `FactoryFor<PKeyState>` property, giữ `VtContextId` |
| `ic->capabilityFlags()` | `Preedit`, `Password`, `SurroundingText`, `Sensitive`… |
| `ic->program()` | app identity |

**Action:**
- `UpdatePreedit`: `Text t; t.append(text, TextFormatFlag::Underline); t.setCursor(byte offset UTF-8)`; nếu `Preedit` capability ⇒ `inputPanel().setClientPreedit(t)` else `setPreedit(t)`; `ic->updatePreedit(); ic->updateUserInterface(UserInterfaceComponent::InputPanel)`.
- `Commit`: `ic->commitString(text)` rồi xóa preedit.
- Lưu ý: `Text::setCursor` dùng **byte offset UTF-8** [VERIFY] ⇒ `Caret.utf8`.

**POC exit (LIN-001/002):** GNOME Wayland + IBus gõ được trong gedit/GNOME Text Editor và Firefox; KDE Wayland + Fcitx5 gõ được trong Kate và Chromium (`--enable-wayland-ime`).

### 14.6 Ma trận capability → hành vi

| Capability thiếu | Hành vi |
|---|---|
| `PREEDIT` | Linux: preedit trong panel; Windows/macOS hầu như luôn có. Không chuyển sang backspace giả. |
| `SURROUNDING_TEXT` | F018/F020 tắt |
| `FOCUS_METADATA` | Dùng profile global |
| `SECURE_INPUT` = true | English pass-through, macro off |

---

## 15. Đồng bộ trạng thái & IPC (`ime-ipc`)

### 15.1 Phân loại dữ liệu
| Loại | Ví dụ | Kênh | Được chứa text gõ? |
|---|---|---|---|
| Config bền | scheme, profiles, macros | File (atomic write) + file watch / generation counter | Không (macro là dữ liệu người dùng nhưng không phải text gõ; không log) |
| Runtime state | VI/EN global, paused-until | State bus | Không |
| Lệnh điều khiển | "reload", "open settings", "doctor query" | IPC message | Không |

### 15.2 State bus theo OS
| OS | Cơ chế đề xuất | Ghi chú |
|---|---|---|
| Windows | TSF global compartment `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` cho VI/EN (TIP đọc/ghi qua `ITfThreadMgr::GetGlobalCompartment`); tray đọc trạng thái qua named shared memory `Local\pKey-State` do TIP cập nhật | [VERIFY] AppContainer ghi được shared memory hay không; nếu không, tray chỉ hiển thị mode theo hotkey của chính nó |
| macOS | Trong process IMK; Settings ↔ IMK qua `NSDistributedNotificationCenter` (chỉ tên lệnh) | |
| Linux | Trong process IBus/Fcitx; Settings ↔ engine qua file watch; Fcitx5 có thể dùng D-Bus addon riêng (tùy chọn) | |

### 15.3 IPC message (settings/tray/cli ↔ coordinator)
- Transport: Windows named pipe `\\.\pipe\pkey-<user SID>` với DACL chỉ user hiện tại; Unix domain socket `$XDG_RUNTIME_DIR/pkey.sock` (0600) / `~/Library/Application Support/pKey/run/pkey.sock`.
- Framing: length-prefixed (u32 LE) JSON, tối đa 64 KiB/message.
- Envelope: `{ "v": 1, "id": 42, "type": "...", "payload": {...} }`.
- Messages v1: `Hello{client, version}`, `GetStatus` → `Status{mode, scheme, backend, ime_registered, ime_selected, paused_until}`, `SetMode{mode}`, `ReloadConfig`, `Pause{minutes}`, `DoctorReport` → báo cáo đã redact.
- Phiên bản: server từ chối `v` lớn hơn hỗ trợ với `Error{code:"unsupported_version"}`.
- **Cấm** mọi message chứa composition/commit text (enforced bằng type system: không có field kiểu text tự do ngoài tên app trong DoctorReport, và test kiểm tra schema).

---

## 16. Cấu hình (`ime-config`)

### 16.1 Vị trí
| OS | Config | Runtime |
|---|---|---|
| Windows | `%APPDATA%\pKey\config.toml`, `macros.json` | `%LOCALAPPDATA%\pKey\runtime\` (snapshot cho TIP, state.json, logs) |
| macOS | `~/Library/Application Support/pKey/` | `~/Library/Caches/pKey/`, logs `~/Library/Logs/pKey/` |
| Linux | `$XDG_CONFIG_HOME/pkey/` (mặc định `~/.config/pkey/`) | `$XDG_STATE_HOME/pkey/` (logs, state.json), socket ở `$XDG_RUNTIME_DIR` |

### 16.2 Schema đầy đủ v1 (giá trị = mặc định)

```toml
schema_version = 1

[general]
enabled = true
default_mode = "vi"              # "vi" | "en"
mode_scope = "global"            # "global" | "per_app"
scheme = "telex"                 # "telex" | "vni" | "viqr"
encoding = "unicode_nfc"         # "unicode_nfc" | "unicode_nfd" | "tcvn3" | "vni_windows" (1.1)
ui_language = "system"           # "system" | "vi" | "en"
theme = "system"                 # "system" | "light" | "dark"
run_at_login = true
show_onboarding = true

[typing]
tone_style = "modern"            # "modern" | "traditional"
free_marking = true
spell_check = true
auto_restore_invalid = true
restore_key = "esc"              # "esc" | "none"
backspace_mode = "grapheme"      # "grapheme" | "raw_key"
commit_on_focus_loss = true
telex_w_standalone = true
telex_brackets = false
allow_foreign_onsets = false     # F019
quick_telex = false              # F016 (1.1)
quick_end_consonant = false      # F017 (1.1)
auto_capitalize = false          # F018 (1.1)

[hotkeys]
toggle_mode = "Ctrl+Shift"       # modifier-only hoặc chord, vd "Alt+Z"
cycle_scheme = ""                # rỗng = tắt
open_settings = ""

[macros]
enabled = true
expand_on = ["space", "enter", "tab", "punctuation"]
max_expansion_chars = 4096

[compat]
preedit_underline = true
windows_hook_fallback = false    # F014 (1.1)
windows_hook_apps = []
linux_surrounding_text = false   # F020

[privacy]
diagnostics_level = "basic"      # "off" | "basic" | "verbose" (verbose vẫn không có text)

[update]
channel = "stable"               # "stable" | "beta" (1.1)
check_automatically = false

[[profiles]]
id = "c2b7…"                     # UUID
name = "VS Code"
enabled = true
[profiles.match]
kind = "exe_name"                # "exe_name" | "process_path" | "aumid" | "bundle_id" | "program" | "ibus_client"
value = "code.exe"
pattern = "exact"                # "exact" | "glob"
[profiles.override]              # mọi khóa đều tùy chọn
mode = "en"
scheme = "telex"
encoding = "unicode_nfc"
tone_style = "modern"
spell_check = true
macros_enabled = false
game_mode = false
```

### 16.3 Hotkey
- Cú pháp: `Mod(+Mod)*(+Key)?`, Mod ∈ `Ctrl Shift Alt Meta`, Key là tên phím (`A`–`Z`, `Space`, `F1`…). So khớp theo `physical_code` (không phụ thuộc layout).
- Modifier-only (vd `Ctrl+Shift`): kích hoạt khi **nhả** phím modifier cuối cùng mà giữa chừng không có phím nào khác ⇒ không xung đột với `Ctrl+Shift+T`.
- Mặc định đề xuất: Windows/Linux `Ctrl+Shift`; macOS `Ctrl+Shift` [VERIFY không xung đột hệ thống; phương án `Ctrl+Option+Space`]. Settings PHẢI phát hiện xung đột với phím chuyển input source của OS (Windows `Ctrl+Shift`/`Win+Space`, macOS `Ctrl+Space`) và cảnh báo.

### 16.4 Load/Save
- Load: parse TOML → validate (`validator` tự viết: enum hợp lệ, độ dài, trùng id) → migrate nếu `schema_version` cũ (chuỗi hàm `migrate_vN_to_vN+1`, có test cho từng bước) → `Arc<Config>`.
- Lỗi parse: giữ bản đang chạy (nếu có), nếu không dùng default; đổi tên file hỏng thành `config.toml.corrupt-<timestamp>`; báo trong Settings.
- Save atomic: ghi `config.toml.tmp` → `fsync` → rename (Windows `ReplaceFileW`/`MoveFileExW(MOVEFILE_REPLACE_EXISTING)`); giữ 5 backup xoay vòng `backups/config-<timestamp>.toml`.
- Khóa khóa không biết (unknown keys): giữ nguyên khi round-trip (dùng `toml_edit`) để không xóa cấu hình từ bản mới hơn.

---

## 17. UI/UX

### 17.1 Công nghệ
- Settings: Slint (Rust), cửa sổ đơn, process riêng `pkey-settings`.
- Tray: Windows dùng `tray-icon` + `muda` trong `pkey-tray.exe` (autostart per-user). macOS: menu của IMK (biểu tượng input menu) — không cần tray riêng. Linux: indicator của IBus/Fcitx5 (properties/status area); tray SNI riêng là tùy chọn.
- i18n: Fluent (`fluent-bundle`), file `assets/i18n/{vi,en}.ftl`.
- Icon: trạng thái VI/EN dạng chữ "V"/"E" (light/dark/high-contrast), tự thiết kế, không dùng icon EVKey.

### 17.2 Menu tray/IME
`Tiếng Việt (V) / English (E)` · `Kiểu gõ ▸ Telex | VNI | VIQR` · `Bảng mã ▸ Unicode dựng sẵn | Unicode tổ hợp` · `Profile: <tên> (chỉ đọc)` · `Tạm dừng ▸ 15 phút | 1 giờ | Đến khi bật lại` · `Cài đặt…` · `Khởi động cùng hệ thống ✓` · `Thoát` (Windows tray; thoát tray không gỡ TIP — TIP vẫn hoạt động theo config).

### 17.3 Settings — các tab và trường
| Tab | Trường |
|---|---|
| Overview | Trạng thái: IME đã đăng ký / đã thêm vào danh sách input / đang được chọn; backend; version; ô gõ thử (text **không** lưu, xóa khi đóng); nút "Mở hướng dẫn kích hoạt" |
| Input | Kiểu gõ, bảng mã, kiểu đặt dấu (có ví dụ live `hoà/hòa`), bỏ dấu tự do, kiểm tra chính tả, tự khôi phục, phím khôi phục, backspace mode, tùy chọn Telex (`w`, `[ ]`) |
| Shortcuts | Toggle VI/EN, cycle scheme, mở settings; ô ghi phím; cảnh báo xung đột |
| Macro | Bảng (trigger, expansion, scope, enabled), tìm kiếm, thêm/sửa/xóa, import/export JSON/CSV, preview import |
| Applications | Danh sách profile; nút "Chọn ứng dụng" (Windows: chọn .exe/process đang chạy; macOS: chọn .app; Linux: nhập program name/chọn từ danh sách đã thấy); overrides; game mode |
| Compatibility | Backend hiện tại; Windows hook fallback (cảnh báo + danh sách app); Linux surrounding text; hướng dẫn xử lý sự cố theo OS |
| System | Khởi động cùng máy, giao diện, ngôn ngữ UI, backup/restore config, reset mặc định |
| About/Doctor | Version, giấy phép bên thứ ba (sinh từ `cargo about`), nút "Tạo báo cáo chẩn đoán" (preview trước khi lưu/copy) |

### 17.4 Onboarding (first-run)
1. Chào mừng + cam kết quyền riêng tư (1 màn hình).
2. Chọn kiểu gõ (Telex/VNI/VIQR) với ví dụ.
3. Kích hoạt IME theo OS — phát hiện trạng thái thật:
   - Windows: kiểm tra profile đã enable trong `ITfInputProcessorProfileMgr`; nút mở `ms-settings:regionlanguage`.
   - macOS: `TISCreateInputSourceList` tìm input source; hướng dẫn System Settings › Keyboard › Input Sources › + › Vietnamese › pKey; nút `TISEnableInputSource` nếu được phép [VERIFY].
   - Linux: phát hiện framework đang chạy (`ibus-daemon` / `fcitx5`, biến `GTK_IM_MODULE`, `QT_IM_MODULE`, `XMODIFIERS`, session Wayland/X11); hướng dẫn `ibus-setup` hoặc `fcitx5-configtool`; cảnh báo cấu hình env thiếu.
4. Gõ thử: người dùng gõ "Tiếng Việt" vào ô test; UI chỉ kiểm tra *kết quả có khớp* rồi xóa ô.
5. Đặt hotkey → Hoàn tất.

Không bao giờ hiển thị "Đang hoạt động" khi bước 3 chưa xác nhận được.

### 17.5 A11y & hiển thị
Điều hướng bàn phím đầy đủ, focus ring, accessible names, tương phản WCAG AA, scale 100–300% DPI/Retina.

---

## 18. CLI (`apps/ime-cli`, binary `pkey-cli`)

```text
pkey-cli simulate --scheme telex [--tone-style modern] [--nfd] "tieengs vieejt"
        # chạy engine với fake host, in kết quả + chuỗi action (dev tool, không đọc stdin ngầm)
pkey-cli golden run [--filter TX] [tests/golden/*.yaml]
pkey-cli config path | show | validate [file] | migrate [file] | reset
pkey macro list | import <file> [--dry-run] | export <file> [--format json|csv]
pkey-cli doctor [--json]          # OS, backend, đăng ký IME, framework env, version, config redacted
pkey status | mode vi|en      # qua IPC
pkey convert --from unicode --to tcvn3 <file>   (1.1)
```
Exit codes: 0 OK, 1 lỗi người dùng, 2 lỗi cấu hình, 3 không kết nối IPC, 70 lỗi nội bộ.

---

## 19. Bảo mật & quyền riêng tư

### 19.1 Threat model (rút gọn)
| Mối đe dọa | Tài sản | Kiểm soát |
|---|---|---|
| Rò rỉ nội dung gõ qua log/crash dump | Văn bản, mật khẩu | Kiểu text không `Debug`; `Redacted<T>`; crash handler không ghi heap dump; test grep log |
| Tiến trình khác đọc/ghi IPC | Config, mode | DACL/0600, kiểm tra peer UID/SID |
| Config độc hại (macro cực lớn, regex DoS) | Tính sẵn sàng | Giới hạn kích thước, không regex người dùng trong v1, glob đơn giản |
| DLL TIP làm crash host | Ứng dụng người dùng | `catch_unwind`, fuzz, soak test, không cấp phát lớn |
| DLL hijacking / thư mục cài ghi được | Toàn hệ thống | Cài vào Program Files (admin), ký code, không nạp DLL từ đường dẫn tương đối |
| Update giả mạo (1.1) | Toàn hệ thống | Manifest ký Ed25519, pin public key, HTTPS, hash |
| Lạm dụng hook fallback | Privacy | Opt-in, theo app, tắt cho elevated/secure desktop |
| Supply chain | Build | `cargo deny`, `cargo audit`, lockfile, SBOM (CycloneDX), review dependency mới |

### 19.2 Kiểm soát bắt buộc
- Engine không mở socket mạng (test bằng `cargo deny` bans cho crate HTTP trong engine crates + kiểm tra runtime).
- Không truy cập clipboard trừ thao tác chủ động của người dùng (F021).
- Không cần admin/root khi chạy; installer dùng quyền theo chuẩn OS.
- Secure input luôn ⇒ pass-through.
- `SECURITY.md`: kênh báo lỗi, chính sách disclosure.

---

## 20. Logging & diagnostics

- `tracing` với subscriber ghi file xoay vòng (≤ 5 × 1 MB) tại thư mục runtime; mặc định level `info`, `diagnostics_level=off` ⇒ chỉ `error`.
- **Được log:** sự kiện vòng đời (activate, focus-in *có/không*, reload config, lỗi API với HRESULT/OSStatus), bộ đếm (số phím xử lý, số commit, số commit fail), thời gian xử lý (histogram), app identity **chỉ khi** `verbose` và người dùng đồng ý.
- **Cấm log:** `KeyInput.text`, composition, commit text, macro trigger/expansion, nội dung surrounding text, clipboard.
- Thực thi: các kiểu chứa text không implement `Debug`/`Display` (hoặc implement in ra `<redacted len=N>`); test integration chạy golden corpus với log `trace` rồi grep file log tìm mọi chuỗi input/expected ⇒ phải không có.
- Crash: Windows TIP không cài crash handler riêng (tránh can thiệp process host); process riêng (settings/tray/IBus) dùng panic hook ghi message + backtrace symbol, không ghi biến.

---

## 21. Hiệu năng

- Hot path `process`: không cấp phát heap cho từ ≤ 16 ký tự (`SmallVec`), bảng vần là `phf`, render NFC bằng bảng tra.
- Benchmarks (`crates/ime-testing/benches`): `process_key_telex`, `process_word_100`, `tone_position`, `render_nfc`, `macro_lookup_10k`.
- CI chạy bench ở chế độ so sánh (không fail theo ngưỡng tuyệt đối trên runner chia sẻ; cảnh báo nếu chậm > 20% so với baseline).
- Đo E2E: adapter ghi timestamp monotonic vào/ra (không text) khi bật env `PKEY_TRACE_LATENCY=1`.
- Kết quả đo ghi vào `docs/perf/<date>-<machine>.md` kèm cấu hình máy.

---

## 22. Chiến lược kiểm thử

### 22.1 Unit
Từng op của từng scheme; parser âm tiết; tone placement (bảng §8.3); validity; render NFC/NFD; caret offsets; case; undo lặp phím; backspace 2 mode; auto restore; macro match/priority; profile resolve; config validate/migrate; hotkey parser & modifier-only logic.

### 22.2 Golden tests
Định dạng `tests/golden/*.yaml`:
```yaml
- id: TX001
  scheme: telex
  options: { tone_style: modern, spell_check: true }
  keys: "tieengs "          # chuỗi phím; ký hiệu đặc biệt: {BS} {ESC} {LEFT} {ENTER} {FOCUSOUT}
  committed: "tiếng "       # văn bản cuối cùng trong fake host
  output_form: NFC
```
Fake host (`ime-testing::FakeHost`) mô phỏng document + composition đúng ngữ nghĩa: áp action, chèn phím pass-through, kiểm tra không có thao tác ngoài composition.

Corpus bắt buộc (≥ 500 case cho 1.0): mọi vần hợp lệ × 6 thanh (sinh tự động từ `rhymes.toml` cho Telex/VNI/VIQR); các thứ tự gõ dấu khác nhau (`tiesng`, `tieensg`, `tiengse`); tone style; hoa/thường; từ tiếng Anh thông dụng (top 1000) với spell-check bật phải giữ nguyên; URL/email/code (`https://`, `foo_bar()`, `x += 1`); số; backspace/esc/mũi tên giữa từ; focus-out giữa từ; macro.

### 22.3 Property & fuzz
- Invariants (proptest): không panic; I1 `would_consume == consumed`; mọi commit là NFC (khi NFC); chuỗi chỉ gồm a–z không có phím biến đổi ⇒ output = input; FakeHost không bao giờ xóa ngoài composition; reset context A không ảnh hưởng context B.
- `cargo-fuzz` targets: `fuzz_process_keys` (chuỗi KeyInput ngẫu nhiên), `fuzz_config_parse`, `fuzz_macro_import_csv`, `fuzz_ffi` (gọi C ABI với con trỏ/độ dài hợp lệ-biên).
- Stress: 1 triệu chuỗi sinh ngẫu nhiên mỗi đêm (nightly job).

### 22.4 FFI
C harness gọi toàn bộ API, kiểm tra ownership, `VT_ERR_PANIC` (bằng feature `test-panic` cố ý panic), chạy ASan/UBSan; Windows: test COM bằng harness nạp DLL và gọi `DllGetClassObject`.

### 22.5 E2E thủ công/bán tự động (máy thật)
Kịch bản chuẩn cho mỗi (OS, app):
1. Gõ câu chuẩn: `Tiếng Việt có dấu, người Hà Nội; quốc gia — thuở xưa!`
2. Gõ nhanh câu chuẩn 10 lần (≥ 150 WPM bằng công cụ phát phím của OS hoặc người thật).
3. Backspace giữa từ, Esc, mũi tên trái giữa từ, click chuột ra chỗ khác giữa từ.
4. Đổi app (Alt-Tab/Cmd-Tab) giữa từ, quay lại.
5. Toggle VI/EN giữa từ.
6. Ô mật khẩu: không biến đổi.
7. Autocomplete (thanh địa chỉ trình duyệt, IntelliSense).
8. Undo (Ctrl/Cmd+Z) sau commit.
Ghi kết quả `PASS | LIMITED | FAIL | NOT_TESTED` vào `docs/COMPATIBILITY_MATRIX.md` kèm OS build, app version, toolkit, backend, ngày, người test; **không** đính kèm nội dung gõ ngoài câu chuẩn.

Tự động hóa (best-effort): Windows — WinAppDriver/UIAutomation + `SendInput` vào Notepad; macOS — XCUITest/AppleScript với TextEdit; Linux — `xdotool`(X11)/`ydotool`+ headless compositor (weston/cage) cho IBus/Fcitx5 [VERIFY khả thi].

### 22.6 Soak/stress
8 giờ phát phím ngẫu nhiên hợp lệ vào app tham chiếu mỗi OS; theo dõi crash, RSS tăng, handle leak.

---

## 23. CI/CD

### 23.1 Workflows
| Workflow | Trigger | Jobs |
|---|---|---|
| `ci.yml` | PR, push main | `fmt --check`; `clippy --all-targets -D warnings`; `test` (ubuntu, macos, windows); golden; doc build; `cargo deny check`; `cargo audit` |
| `ffi.yml` | PR chạm `ime-ffi`/`platform` | build C harness, ASan/UBSan (Linux, macOS) |
| `platform-build.yml` | PR, push | Windows TSF x86/x64 (MSVC); macOS universal IMK bundle (Xcode); Linux IBus + Fcitx5 (Ubuntu LTS, Fedora container) |
| `nightly.yml` | cron | fuzz 30 phút/target, stress 1M, bench so sánh |
| `release.yml` | tag `v*` | build, sign, notarize, package, SBOM, checksums, GitHub Release draft |

### 23.2 Quy tắc
- Branch `main` được bảo vệ; PR cần CI xanh + 1 review.
- Commit nhỏ, build độc lập; mỗi PR có spec trong `docs/specs/` (theo quy ước dự án).
- Secrets ký code chỉ có ở job release trên môi trường được bảo vệ.

---

## 24. Đóng gói & cài đặt

### 24.1 Windows
- Installer: WiX v4 MSI (per-machine) — chứa `pKey.TSF.dll` (x64 vào `Program Files\pKey\x64\`, x86 vào `Program Files\pKey\x86\` hoặc `Program Files (x86)`), `pkey-tray.exe`, `pkey-settings.exe`, `pkey.exe`.
- Custom action: đăng ký COM/TIP cho cả hai kiến trúc; set ACL đọc cho `ALL APPLICATION PACKAGES` trên thư mục cài.
- Autostart tray: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (do app ghi khi người dùng bật, không phải installer ép).
- Gỡ cài: unregister TIP (profile + category + COM), xóa file; hỏi xóa config. DLL đang bị nạp trong process khác ⇒ đánh dấu xóa sau reboot (`MoveFileEx` delay) và thông báo.
- Nâng cấp: MSI major upgrade; DLL đang khóa ⇒ cài DLL tên có version (`pKey.TSF.1.2.0.dll`) và cập nhật đường dẫn COM [VERIFY phương án tránh reboot].
- Ký: Authenticode (EV hoặc OV), timestamp.

### 24.2 macOS
- `.pkg` (productbuild) cài `pKey.app` vào `/Library/Input Methods/` và `pKey Settings.app` vào `/Applications/`.
- Postinstall: không tự enable input source mà không có hành động người dùng; mở Settings onboarding.
- Gỡ: script `uninstall.sh` + nút trong Settings (yêu cầu quyền admin cho `/Library`), xóa input source khỏi TIS, hỏi xóa config.
- Ký + notarize + staple; universal binary.

### 24.3 Linux
- Gói: `pkey-common` (core lib, assets, CLI), `pkey-ibus`, `pkey-fcitx5`, `pkey-settings`. Định dạng `.deb` (Ubuntu 22.04/24.04, Debian 12) và `.rpm` (Fedora hiện hành); Arch PKGBUILD cộng đồng (tùy chọn).
- Post-install: IBus — `ibus write-cache`/hướng dẫn `ibus restart`; Fcitx5 — hướng dẫn restart fcitx5.
- Không sửa `/etc/environment` tự động; doctor phát hiện thiếu `GTK_IM_MODULE`/`QT_IM_MODULE`/`XMODIFIERS` và hướng dẫn.
- Flatpak/AppImage chỉ cho Settings, không cho engine.

### 24.4 Release
SemVer; kênh `stable`/`beta`; changelog (Keep a Changelog); SHA-256 checksums + chữ ký minisign cho mọi artifact; SBOM đính kèm.

---

## 25. Cập nhật (F015, 1.1)
- `pkey-updater` process riêng, chạy khi người dùng bấm "Kiểm tra" hoặc theo lịch nếu `check_automatically=true`.
- Manifest JSON `{version, channel, min_os, artifacts:[{os, arch, url, sha256, size}], notes_url}` ký Ed25519; public key nhúng trong binary; từ chối downgrade trừ rollback chủ động.
- Tải về thư mục tạm user, kiểm hash + chữ ký OS (Authenticode / codesign) trước khi chạy installer.
- Rollback: giữ installer bản trước; nút "Quay lại phiên bản trước".
- Không gửi bất kỳ dữ liệu nào ngoài version/OS/arch trong request.

---

## 26. Roadmap & milestone

Kế hoạch chi tiết (work package, ước lượng pd, gate, lịch ví dụ) nằm ở [IMPLEMENTATION_PHASES.md](IMPLEMENTATION_PHASES.md). Bảng dưới là tóm tắt; nếu hai tài liệu khác nhau, IMPLEMENTATION_PHASES.md được ưu tiên.

| Phase | Nội dung | Tuần | Gate / mốc |
|---|---|---|---|
| **P0 Foundation & Discovery** | Repo/CI, license, ADR-001..006, 4 POC native | W1–W4 | G0 |
| **P1 Core engine** | Unicode, ngữ âm, Telex, VNI, state machine, config, macro core, FFI, CLI, golden ≥ 300 | W2–W9 | G1 |
| **P2 Windows vertical slice** | TSF đầy đủ, config snapshot, toggle, tray tối thiểu, MSI dev | W5–W12 | G2 → Alpha-Win |
| **P3 macOS + Linux adapters** | IMK, IBus, Fcitx5 đầy đủ + E2E | W8–W16 | G3 |
| **P4 UX & tính năng** | Settings, tray/menu, onboarding, macro/profile UI, VIQR, doctor | W10–W18 | G4 → Beta (W19) |
| **P5 Hardening & 1.0** | Security, compat matrix, soak, installer ký, docs | W18–W23 | G5 → 1.0 GA |
| **P6 Release 1.1** | TCVN3/VNI-Win, updater, hook fallback, Quick Telex | W24–W31 | G6 |

### 26.1 Backlog chi tiết (issue seeds mở rộng)

| Issue | Deliverable | Phụ thuộc | Phase |
|---|---|---|---|
| ADR-001 | Core architecture & event contract | — | P0 |
| ADR-002 | Windows TSF (Rust `windows` crate vs C++ shim; x86 TIP; AppContainer config) | — | P0 |
| ADR-003 | macOS IMK (Swift vs ObjC shim, bundle layout) | — | P0 |
| ADR-004 | Linux IBus (C shim vs zbus) & Fcitx5 | — | P0 |
| ADR-005 | State bus & IPC | ADR-001 | P0 |
| ADR-006 | Settings UI (Slint) & tray stack | — | P0 |
| WIN-001 | TSF POC | ADR-002 | P0 |
| MAC-001 | IMK POC | ADR-003 | P0 |
| LIN-001 | IBus POC | ADR-004 | P0 |
| LIN-002 | Fcitx5 POC | ADR-004 | P0 |
| CORE-001 | Kiểu dữ liệu, event protocol, capability | ADR-001 | P1 |
| CORE-002 | `ime-unicode`: render NFC/NFD, caret offsets | CORE-001 | P1 |
| CORE-003 | `rhymes.toml` + parser + validity + tone placement | CORE-002 | P1 |
| CORE-004 | Telex rules | CORE-003 | P1 |
| CORE-005 | VNI rules | CORE-003 | P1 |
| CORE-006 | State machine, contexts, backspace, restore | CORE-001 | P1 |
| CORE-007 | Hotkey parser & modifier-only detection | CORE-001 | P1 |
| CORE-008 | `ime-config` schema, validate, migrate, atomic save | — | P1 |
| CORE-009 | `ime-macro` | CORE-002 | P1 |
| CORE-010 | `ime-ffi` + cbindgen + C harness | CORE-006 | P1 |
| CORE-011 | `ime-testing`: FakeHost, golden runner, proptest, fuzz | CORE-006 | P1 |
| CLI-001 | `pkey-cli simulate/golden/config` | CORE-011 | P1 |
| WIN-002 | TSF đầy đủ: composition, display attribute, lang bar, compartment | WIN-001, CORE-006 | P2 |
| WIN-003 | Config snapshot + AppContainer ACL + reload | WIN-002, CORE-008 | P2 |
| MAC-002 | IMK đầy đủ: marked text, menu, reload | MAC-001, CORE-010 | P3 |
| LIN-003 | IBus đầy đủ: preedit mode, properties, content type | LIN-001 | P3 |
| LIN-004 | Fcitx5 đầy đủ: per-IC state, capability, status | LIN-002, CORE-010 | P3 |
| CORE-012 | Profile resolve + app identity mapping | CORE-008 | P2 |
| IPC-001 | `ime-ipc` + state bus từng OS | ADR-005 | P2 |
| CORE-013 | VIQR | CORE-003 | P4 |
| UI-001 | Settings shell + Overview + Input | IPC-001 | P4 |
| UI-002 | Shortcuts, Macro, Applications tabs | UI-001 | P4 |
| UI-003 | Tray Windows; menu IMK; IBus/Fcitx indicator | IPC-001 | P4 |
| UI-004 | Onboarding + detection từng OS | UI-001 | P4 |
| DIAG-001 | Doctor (CLI + UI), redaction tests | IPC-001 | P4 |
| SEC-001 | Threat model, privacy tests, FFI security review | Tất cả adapter | P5 |
| QA-001 | Compatibility matrix thực tế | Tất cả adapter | P5 |
| QA-002 | Soak 8h mỗi OS | QA-001 | P5 |
| REL-001 | Installers + signing + release pipeline | QA-001 | P5 |
| DOC-001 | Hướng dẫn người dùng từng OS, FAQ, known limitations | REL-001 | P5 |

---

## 27. Rủi ro & giảm thiểu

| Rủi ro | Mức | Giảm thiểu |
|---|---|---|
| TSF từ Rust thuần phức tạp (COM vtable, threading) | Cao | POC sớm; cho phép C++ shim mỏng theo ADR-002; tham khảo mẫu TSF chính thức của Microsoft |
| TIP x86 + x64 + ARM64 tăng chi phí build/test | Trung bình | Ưu tiên x64 + x86; ARM64 sau 1.0 |
| AppContainer không đọc được config | Trung bình | POC WIN-003; fallback default compiled-in |
| Ứng dụng xử lý preedit kém (Electron cũ, terminal, game) | Cao | Ma trận compat trung thực; profile EN mặc định đề xuất cho app biết lỗi; hook fallback 1.1 cho Windows |
| Wayland phân mảnh (text-input v1/v3, compositor) | Cao | Chỉ công bố cấu hình đã test (GNOME+IBus, KDE+Fcitx5) |
| macOS thay đổi IMK/TIS giữa các phiên bản | Trung bình | Test trên 2 phiên bản macOS gần nhất mỗi release |
| Luật ngữ âm thiếu case hiếm (khuỷu, quều, oong…) | Trung bình | `rhymes.toml` data-driven, corpus mở rộng, issue template báo từ sai |
| Rò rỉ text qua log do dev vô ý | Cao | Kiểu dữ liệu không Debug, test grep log, review checklist |
| Chi phí chứng chỉ ký code | Thấp–Trung bình | Lập ngân sách từ P0; build không ký cho beta nội bộ |
| Rủi ro pháp lý khi "học hành vi" EVKey | Thấp | Clean-room log: chỉ tài liệu tính năng công khai; không xem/dịch ngược binary |

---

## 28. ADR & câu hỏi mở

| # | Câu hỏi | Cách giải quyết |
|---|---|---|
| Q1 | Tên sản phẩm chính thức, bundle id, publisher, license dự án (MIT/Apache-2.0/GPL?) | Quyết định trước P1 (ảnh hưởng dependency GPL như libibus là LGPL — tương thích) |
| Q2 | Hotkey mặc định trên macOS | Usability test P3 |
| Q3 | IBus: C shim hay zbus thuần | Kết quả LIN-001 |
| Q4 | Windows: cơ chế cập nhật DLL đang bị nạp không cần reboot | WIN-003/REL-001 |
| Q5 | Preedit khi encoding legacy (TCVN3) hiển thị gì | Thiết kế F013 |
| Q6 | Có cho phép người dùng tự định nghĩa kiểu gõ không | Sau 1.1 |
| Q7 | `GUID_TFCAT_TIPCAP_SECUREMODE`: có nên khai báo | Security review P4 |
| Q8 | Commit `\n` trong macro trên từng backend | Test P2 |

---

## 29. Checklist chấp nhận release 1.0

**Core**
- [ ] Golden ≥ 500 case pass trên 3 OS; cùng input + config ⇒ cùng output byte-for-byte.
- [ ] Fuzz 24h tích lũy không crash; proptest invariants pass.
- [ ] Bench p95 < 1 ms trên máy tham chiếu (ghi cấu hình).

**Windows**
- [ ] TIP đăng ký/chọn được; gõ trong Notepad, Word, Chrome, Edge, VS Code, Windows Terminal; app 32-bit.
- [ ] Không crash host trong soak 8h; edit session fail ⇒ fail-open.
- [ ] Cài/nâng cấp/gỡ sạch trên máy mới.

**macOS**
- [ ] Input source được nhận; gõ trong TextEdit, Notes, Safari, Chrome, VS Code; arm64 + x86_64.
- [ ] Ký + notarize; không yêu cầu Accessibility/Input Monitoring.

**Linux**
- [ ] IBus trên GNOME Wayland (Ubuntu 24.04); Fcitx5 trên KDE Wayland (Fedora KDE hoặc Kubuntu); X11 cho ít nhất một backend.
- [ ] Caveats Wayland ghi trong docs.

**Privacy & security**
- [ ] Test grep log/diagnostics không chứa text gõ.
- [ ] Không socket mạng từ engine; IPC chỉ same-user.
- [ ] `cargo deny` + `cargo audit` sạch; SBOM; license bên thứ ba trong About.

**UX**
- [ ] Onboarding phát hiện đúng trạng thái kích hoạt trên cả 3 OS.
- [ ] Settings vi/en, a11y checklist đạt.

**Truthfulness**
- [ ] `COMPATIBILITY_MATRIX.md` cập nhật; không quảng cáo hỗ trợ game/Wayland/password field ngoài kết quả test.

---

## Phụ lục A — Bảng nguyên âm có dấu (NFC)

| Base | Không | Sắc | Huyền | Hỏi | Ngã | Nặng |
|---|---|---|---|---|---|---|
| a | a | á | à | ả | ã | ạ |
| ă | ă | ắ | ằ | ẳ | ẵ | ặ |
| â | â | ấ | ầ | ẩ | ẫ | ậ |
| e | e | é | è | ẻ | ẽ | ẹ |
| ê | ê | ế | ề | ể | ễ | ệ |
| i | i | í | ì | ỉ | ĩ | ị |
| o | o | ó | ò | ỏ | õ | ọ |
| ô | ô | ố | ồ | ổ | ỗ | ộ |
| ơ | ơ | ớ | ờ | ở | ỡ | ợ |
| u | u | ú | ù | ủ | ũ | ụ |
| ư | ư | ứ | ừ | ử | ữ | ự |
| y | y | ý | ỳ | ỷ | ỹ | ỵ |

Cộng `đ/Đ` ⇒ 67 chữ thường + 67 chữ hoa = 134 ký tự.

## Phụ lục B — Golden corpus mẫu

```yaml
# Telex
- { id: TX001, scheme: telex, keys: "tieengs ",   committed: "tiếng " }
- { id: TX002, scheme: telex, keys: "ddawng ",    committed: "đăng " }
- { id: TX003, scheme: telex, keys: "caay ",      committed: "cây " }
- { id: TX004, scheme: telex, keys: "nguwowif ",  committed: "người " }
- { id: TX005, scheme: telex, keys: "nguowif ",   committed: "người " }
- { id: TX006, scheme: telex, keys: "quoocs ",    committed: "quốc " }
- { id: TX007, scheme: telex, keys: "giaf ",      committed: "già " }
- { id: TX008, scheme: telex, keys: "gif ",       committed: "gì " }
- { id: TX009, scheme: telex, keys: "hoaf ",      committed: "hoà ", options: { tone_style: modern } }
- { id: TX010, scheme: telex, keys: "hoaf ",      committed: "hòa ", options: { tone_style: traditional } }
- { id: TX011, scheme: telex, keys: "thuyr ",     committed: "thuỷ ", options: { tone_style: modern } }
- { id: TX012, scheme: telex, keys: "khuyeens ",  committed: "khuyến " }
- { id: TX013, scheme: telex, keys: "ruwowuj ",   committed: "rượu " }
- { id: TX014, scheme: telex, keys: "tuooir ",    committed: "tuổi " }
- { id: TX015, scheme: telex, keys: "aaa ",       committed: "aa " }
- { id: TX016, scheme: telex, keys: "ass ",       committed: "as " }
- { id: TX017, scheme: telex, keys: "DDAWNG ",    committed: "ĐĂNG " }
- { id: TX018, scheme: telex, keys: "windows ",   committed: "windows " }        # auto restore
- { id: TX019, scheme: telex, keys: "tieengs{BS} ", committed: "tiến " }         # backspace grapheme
- { id: TX020, scheme: telex, keys: "tieengs{ESC}", committed: "tieengs" }       # restore
- { id: TX021, scheme: telex, keys: "tapf ",      committed: "tapf " }           # coda tắc + huyền ⇒ invalid ⇒ restore
- { id: TX022, scheme: telex, keys: "xoawn ",     committed: "xoăn " }
- { id: TX023, scheme: telex, keys: "quow ",      committed: "quơ " }
- { id: TX024, scheme: telex, keys: "tiesng ",    committed: "tiéng ", options: { spell_check: false } }  # tone đặt trước phụ âm cuối
# VNI
- { id: VN001, scheme: vni, keys: "tie61ng ",     committed: "tiếng " }
- { id: VN002, scheme: vni, keys: "d9i ",         committed: "đi " }
- { id: VN003, scheme: vni, keys: "nguo72i ",     committed: "người " }
- { id: VN004, scheme: vni, keys: "a11 ",         committed: "a1 " }
- { id: VN005, scheme: vni, keys: "mp3 ",         committed: "mp3 " }
# VIQR
- { id: VQ001, scheme: viqr, keys: "a^ ",         committed: "â " }
- { id: VQ002, scheme: viqr, keys: "tie^'ng ",    committed: "tiếng " }
- { id: VQ003, scheme: viqr, keys: "d-i ",        committed: "đi " }
- { id: VQ004, scheme: viqr, keys: "ddi ",        committed: "đi " }
- { id: VQ005, scheme: viqr, keys: "co\\? ",      committed: "co? " }
```
Ghi chú TX024: tone thuộc âm tiết, không thuộc vị trí phím ⇒ gõ `s` trước `ng` vẫn cho cùng kết quả; vị trí dấu được tính lại mỗi phím. `tiéng` không có mũ (chưa gõ `ee`) và không phải âm tiết hợp lệ, nên fixture tắt `spell_check` để tránh auto-restore. Khi `spell_check=true`, kết quả mong đợi là restore về `tiesng` — thêm fixture riêng trong CORE-004.

## Phụ lục C — Mapping phím chuẩn hóa

| Logical | Windows VK | macOS keyCode | X11/Wayland keysym |
|---|---|---|---|
| Backspace | `VK_BACK` | 51 | `BackSpace` |
| Delete | `VK_DELETE` | 117 | `Delete` |
| Enter | `VK_RETURN` | 36 / 76 | `Return` / `KP_Enter` |
| Tab | `VK_TAB` | 48 | `Tab` |
| Escape | `VK_ESCAPE` | 53 | `Escape` |
| Space | `VK_SPACE` | 49 | `space` |
| Left/Right/Up/Down | `VK_LEFT…` | 123/124/126/125 | `Left`… |
| Home/End | `VK_HOME/VK_END` | 115/119 | `Home/End` |
| PageUp/PageDown | `VK_PRIOR/VK_NEXT` | 116/121 | `Prior/Next` |

Ký tự (`text`) lấy từ: Windows `ToUnicodeEx` với layout hiện tại (không làm hỏng dead-key state [VERIFY]: dùng flag `0x4` trên Windows 10 1607+), macOS `[event characters]`, IBus keyval → unicode, Fcitx5 `Key::keySymToUnicode`.

## Phụ lục D — Tài liệu tham khảo
- Spec gốc: [SPEC_pKey_Rust_CrossPlatform.md](SPEC_pKey_Rust_CrossPlatform.md) §17
- Microsoft TSF: https://learn.microsoft.com/en-us/windows/win32/tsf/text-services-framework
- Microsoft IME requirements: https://learn.microsoft.com/en-us/windows/apps/develop/input/input-method-editor-requirements
- Microsoft SendInput / UIPI: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- Apple InputMethodKit: https://developer.apple.com/documentation/inputmethodkit
- IBus: https://github.com/ibus/ibus · IBusEngine API: https://ibus.github.io/docs/ibus-1.5/IBusEngine.html
- Fcitx5 develop: https://fcitx-im.org/wiki/Develop_an_simple_input_method/en · Wayland: https://fcitx-im.org/wiki/Using_Fcitx_5_on_Wayland/en
- Unicode UAX #15: https://www.unicode.org/reports/tr15/
- EVKey (chỉ tài liệu hành vi công khai): https://github.com/lamquangminh/EVKey
