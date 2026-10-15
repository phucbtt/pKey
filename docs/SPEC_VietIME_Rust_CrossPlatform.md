# SPEC — VietIME Rust: Bộ gõ tiếng Việt đa nền tảng

**Version:** 1.0 | **Ngày:** 2026-10-15 | **Trạng thái:** Đặc tả đề xuất; chưa triển khai/benchmark  
**Đích:** Windows, macOS, Linux | **Ngôn ngữ lõi:** Rust stable  
**Reference:** https://github.com/lamquangminh/EVKey

> **Phân biệt nguồn và thiết kế:** EVKey công khai chứa tài liệu tính năng, các binary và mã C++ updater `evkau` nhưng không chứa đầy đủ mã nguồn engine. Không có license root xác nhận quyền tái sử dụng toàn bộ. Dự án này **clean-room**: học hành vi/tính năng, tự viết mới, không sao chép binary/branding hoặc mã chưa rõ quyền sử dụng. Mọi ngưỡng hiệu năng bên dưới là **mục tiêu**, không phải số đo thực tế.

## 1. Tổng quan sản phẩm

**Tên làm việc:** VietIME Rust. **Loại:** desktop Vietnamese Input Method Editor (IME), offline-first. **Mục tiêu:** gõ Telex/VNI/VIQR, Unicode NFC/NFD, gõ dấu cũ/mới, chuyển Việt–Anh, macro, profiles theo ứng dụng, tray/menu, tự khởi động và trải nghiệm ổn định. Có thể mở rộng các bảng mã cũ TCVN3, VNI Windows.

Nguyên tắc: (1) dùng chung thuật toán Rust; (2) mỗi OS dùng **native input method adapter**; (3) IME engine hoạt động dù Settings UI đóng; (4) không ghi nội dung người dùng gõ, không truyền dữ liệu gõ lên mạng; (5) không giả định có thể hook/phát lại keystroke trên mọi ứng dụng/Wayland/secure field; (6) fail-open khi adapter bị lỗi, không xóa ngoài composition.

**Ngoài phạm vi v1:** AI/cloud autocorrect, tài khoản/sync, keylogger, tự nhập trên màn hình đăng nhập, vượt UAC/anti-cheat/sandbox, đảm bảo 100% game và ứng dụng tự viết text control.

## 2. Danh sách yêu cầu chức năng

| ID | Feature | Priority | Version | Tiêu chí hoàn thành |
|---|---|---|---|---|
| F001 | Telex, VNI | P0 | MVP | Golden tests và typing native thông qua adapter |
| F002 | Unicode NFC, NFD | P0 | MVP | Normalization nhất quán, UTF-16/UTF-8 offsets đúng API |
| F003 | Chuyển VI/EN | P0 | MVP | Hotkey/toggle, pass-through khi EN, state hiển thị đúng |
| F004 | Native IME | P0 | MVP–1.0 | Windows TSF, macOS InputMethodKit, Linux IBus/Fcitx5 |
| F005 | Composition | P0 | MVP | Preedit, commit, caret, backspace, focus switch, recovery |
| F006 | Tray/menu | P0 | 1.0 | VI/EN, schemes, Settings, pause, quit |
| F007 | Input options | P1 | 1.0 | Dấu cũ/mới, kiểm tra chính tả tùy chọn, phục hồi phím |
| F008 | VIQR | P1 | 1.0 | Dấu/punctuation disambiguation, escape |
| F009 | Macro | P1 | 1.0 | CRUD, scopes, import/export và bảo vệ secure text |
| F010 | Per-app profile | P1 | 1.0 | Nhận diện app, rule precedence, game mode |
| F011 | Startup/onboarding | P1 | 1.0 | IME setup & verification; uninstaller sạch |
| F012 | Diagnostics | P1 | 1.0 | Kiểm tra OS/backends không log phím |
| F013 | TCVN3/VNI Windows | P2 | 1.1 | Mapping kiểm chứng, convert text có xác nhận |
| F014 | Windows hook fallback | P2 | 1.1 | Opt-in, loop guard, không vượt UIPI |
| F015 | Signed updates | P2 | 1.1 | Signature, hash, rollback, staged release |

## 3. Kiến trúc và giới hạn theo OS

### Windows
- **Bắt buộc:** TSF Text Service / COM IME cho luồng chính. Implement từ Rust qua `windows` crate; C++ bridge rất mỏng được phép nếu chứng minh cần thiết. Đăng ký text service/input profile đúng chuẩn; không tự ghi registry để ép đổi input mặc định. Tách COM component khỏi GUI.
- **Tùy chọn fallback:** `WH_KEYBOARD_LL` + `SendInput`, tắt mặc định; xử lý nhanh, không đơ OS, đánh dấu synthetic events chống vòng lặp. `SendInput` bị ràng buộc bởi UIPI; không bảo đảm nhập vào tiến trình nâng quyền. Không bypass UAC hoặc anti-cheat.
- **Test:** Notepad, Office, Chrome/Edge, VS Code/Electron, WinUI/WPF, terminal; x64 trước, ARM64 sau khi kiểm chứng.

### macOS
- **Bắt buộc:** InputMethodKit (`IMKServer`/`IMKInputController`), Objective-C/Swift shim gọi Rust C ABI; quản lý text composition và marked text theo phiên input.
- App/Input Method bundle phải có Info.plist/đăng ký Input Source, signing/notarization cho bản phân phối. Hướng dẫn người dùng bật input source.
- Không coi CGEventTap là backend chính; chỉ thêm khi có tính năng riêng có consent và quyền OS. Native InputMethodKit không mặc nhiên đồng nghĩa cần Input Monitoring/Accessibility.
- **Test:** TextEdit, Notes, Safari, Chrome, VS Code, Terminal; Apple Silicon/Intel; secure-input/password và app tự quản lý composition.

### Linux
- **IBus:** viết IBus engine với GLib/DBus + Rust; nhận key event, commit/preedit, focus/reset, capability.
- **Fcitx5:** viết addon shared library theo C++ API Fcitx, gọi Rust engine bằng C ABI; cài `.so` và `.conf` đúng vị trí.
- Dùng input method framework thay vì `/dev/input`, root hoặc generic X11 hook; Wayland không cung cấp global interception kiểu X11. Hỗ trợ phụ thuộc compositor, toolkit, các text-input protocol, config environment và XWayland/native Wayland.
- **Test:** GNOME Wayland + IBus, KDE Wayland + Fcitx5, X11 + IBus/Fcitx5, GTK/Qt/Chromium/Electron. Không cam kết mọi distro/compositor nếu chưa test.

**Nguồn:** Microsoft TSF và IME requirements; Apple InputMethodKit; IBus; Fcitx5 developer guide và Wayland support (dẫn nguồn ở mục 19).

## 4. Rust workspace

```text
vietime/
├── Cargo.toml
├── crates/
│   ├── ime-core/          # state machine, normalized events, capabilities
│   ├── ime-vietnamese/    # syllable parser, tone position, case
│   ├── ime-telex/         # telex mapping
│   ├── ime-vni/           # vni mapping
│   ├── ime-viqr/          # viqr mapping
│   ├── ime-unicode/       # NFC/NFD, grapheme, future legacy mapping
│   ├── ime-macro/         # expansions; no script execution
│   ├── ime-config/        # preferences, app profiles, migrations
│   ├── ime-ffi/           # stable C ABI, panic isolation
│   ├── ime-ipc/           # versioned local IPC settings/engine only
│   └── ime-testing/       # fixtures, fake host, benchmarks
├── platform/
│   ├── windows-tsf/
│   ├── windows-hook/      # optional; disabled by default
│   ├── macos-imk/
│   ├── linux-ibus/
│   └── linux-fcitx5/
├── apps/
│   ├── settings/          # Slint UI (settings only)
│   ├── tray/              # native tray/menu wrapper
│   └── ime-cli/           # doctor, test, config, import/export
├── assets/{mappings,icons,fixtures}/
├── installers/{windows,macos,linux}/
├── docs/{ARCHITECTURE,SECURITY,COMPATIBILITY_MATRIX,IME_RULES}.md
├── tests/{golden,integration,e2e}/
└── .github/workflows/
```

**Dependencies ứng viên cần audit version/license:** `windows`, `objc2` (+ Foundation/AppKit), `glib`, `gio`, `zbus`, `unicode-normalization`, `unicode-segmentation`, `smallvec`, `serde`, `toml`, `tracing`, `thiserror`, `slint`, `criterion`, `proptest`, `cargo-fuzz`. Framework UI không thay thế TSF/InputMethodKit/IBus/Fcitx5. C/C++/ObjC bridge được cho phép khi OS ABI đòi hỏi; **Rust-first** thay vì câu chữ “100% Rust”.

## 5. Luồng dữ liệu và contract

```text
Native OS key/context event
      ↓
Adapter: physical → logical key + capability flags
      ↓
Rust core: profile → context → syllable parser → scheme rules
      ↓
EngineAction: PassThrough / UpdatePreedit / Commit / Cancel
      ↓
OS-native IME composition/edit-session API
      ↓
Focused text control

Settings UI ←→ local same-user IPC ←→ config/profile coordinator
```

- Core **pure**: không biết HWND/NSEvent/DBus/PID. Một bộ đệm gõ độc lập cho mỗi input context, reset tại focus/selection move/OS switch.
- Các event cần có: text key, modifiers, physical/logical key, repeat, synthetic flag, backspace, delete, arrow, selection/caret move, paste, focus/blur, composition-canceled, secure field, layout change, mode toggle.
- `EngineAction` mô tả thay đổi composition (không mô tả backspace giả). Adapter chỉ thay văn bản trong phạm vi composition hoặc selection mà OS API cho phép. `Commit`/`UpdatePreedit` phải atomic khi host hỗ trợ.
- Offset caret/selection phải dùng grapheme hoặc UTF-16 code units theo adapter, **không** dùng byte index UTF-8 trên Windows/macOS text API.
- Lifecycle: GUI crash không làm rơi IME; native adapter lỗi thì pass-through/fail-open và reset; UI IPC chỉ truyền setting/state, không gửi văn bản đang gõ.

### API định hướng (pseudocode Rust)

```rust
pub enum InputMode { Vietnamese, English }
pub enum Scheme { Telex, Vni, Viqr }
pub enum OutputEncoding { UnicodeNfc, UnicodeNfd, Tcvn3, VniWindows }

pub struct KeyInput {
    pub context_id: u64,
    pub logical_key: LogicalKey,
    pub text: Option<String>,
    pub modifiers: Modifiers,
    pub is_synthetic: bool,
    pub repeat: bool,
}

pub enum EngineAction {
    PassThrough,
    Consume,
    UpdatePreedit { text: String, caret_grapheme: usize },
    CommitText(String),
    CancelComposition,
    FinishComposition,
}

pub trait VietnameseEngine {
    fn process(&mut self, event: KeyInput) -> Vec<EngineAction>;
    fn reset_context(&mut self, context_id: u64);
    fn set_profile(&mut self, context_id: u64, profile: &Profile);
}
```

`LogicalKey`, `Modifiers`, `Profile` cần định nghĩa thật ở code. API trên chỉ để thể hiện ranh giới, không phải đoạn code hoàn chỉnh. Capability negotiation: `supports_preedit`, `supports_selection_replace`, `supports_surrounding_text`, `secure_input`, `supports_focus_metadata`.

### State machine

```text
IDLE
  Printable → COMPOSING
  Mode/shortcut → update/pass-through
COMPOSING
  shape/tone key → recompute + UpdatePreedit
  ordinary key → append + UpdatePreedit
  Backspace → undo 1 raw-key event or 1 grapheme
  delimiter → commit then delimiter
  caret/selection/focus changed → commit/cancel safely + reset
  Esc → restore literal or cancel per setting
  OS composition canceled → discard local state (no stray deletion)
```

Lưu **raw-key history** độc lập với **displayed composition**; không phục hồi key sequence bằng cách bỏ dấu từ output. Key consumed mà host commit fail phải được xử lý có trạng thái; không replay mù gây double character.

## 6. Quy tắc bộ gõ

### Telex

| Sequence | Output | Ghi chú |
|---|---|---|
| `aa`, `aw`, `ee`, `oo`, `ow`, `uw`, `dd` | `â`, `ă`, `ê`, `ô`, `ơ`, `ư`, `đ` | Tôn trọng case; `w` standalone có setting |
| `s`, `f`, `r`, `x`, `j` | sắc, huyền, hỏi, ngã, nặng | Chỉ trong ngữ cảnh âm tiết hợp lệ |
| `z` | Xóa thanh/shape nếu profile cho phép | Có escape/repeated key fallback |

**Golden examples:** `tieengs` → `tiếng`; `ddawng` → `đăng`; `caay` → `cây`; `as` → `á`; `af` → `à`; `ar` → `ả`; `ax` → `ã`; `aj` → `ạ`.

### VNI

`1` sắc, `2` huyền, `3` hỏi, `4` ngã, `5` nặng, `0` bỏ thanh, `6` mũ (â/ê/ô), `7` móc (ơ/ư), `8` breve (ă), `9` đ. `tie61ng` → `tiếng`; `d9i` → `đi`. Không thay số khi đang gõ code/URL/English hoặc syllable không hợp lệ.

### VIQR

`a^` → â; `a(` → ă; `o+` → ơ; `u+` → ư; `d-` → đ; `'`, `` ` ``, `?`, `~`, `.` là phím thanh trong ngữ cảnh phù hợp. Có escape/literal mode để gõ punctuation, code, Markdown, URL.

### Tone + syllable rules

- Parser `onset + nucleus + coda + tone + case`, xử lý `qu`, `gi`, nguyên âm ghép, dấu đặt muộn, dấu cũ/mới. Unit test các lựa chọn như `hòa/hoà`, `thúy/thuý`, `hủy/huỷ`.
- Nền Unicode normalize: NFC mặc định, NFD khi cấu hình. Không nhầm Unicode tổ hợp với legacy encodings. TCVN3/VNI Windows cần bảng ánh xạ verified riêng.
- Tự phân biệt tiếng Việt hợp lệ và literal English/technical strings theo chế độ spell-check; khi spell-check off vẫn cho người dùng gõ tiếng Việt, không ép dictionary cản tên riêng.
- Phím lặp, backspace, Undo, Esc và bàn phím Caps Lock phải được kiểm thử theo raw event history. Luồng đầu ra không tự sửa ký tự ở ngoài composition.

## 7. Macro, application profile, game mode

### Macro
- Schema: `trigger`, `expansion`, `case_sensitive`, `whole_word`, `enabled`, `scope`, `priority`.
- Expand theo delimiter có cấu hình; quản lý CRUD, search, export/import JSON/CSV UTF-8, backup; chống trùng trigger, giới hạn độ dài output.
- Vô hiệu trong secure fields; không hỗ trợ `exec`/shell script trong v1; tránh kích hoạt không mong muốn ở URL/password/terminal.

### App profile
- Match: Windows app identity/process path/publisher nếu khả dụng; macOS bundle ID; Linux desktop ID/process identity khi khả dụng.
- Override `enabled`, `scheme`, `encoding`, `tone_style`, `spell_check`, `macro_enabled`, `game_mode`, `compatibility_mode`.
- Priority: **security/input capability > explicit app profile > global defaults**. Không tự ý đọc tên process nếu OS không cung cấp/không được cấp quyền; fallback global.
- Game Mode: mặc định pass-through và không chạm vào anti-cheat; nếu game chat input có native capability đã test, có thể cho phép gõ. Không đảm bảo game raw input.

## 8. UI/UX

**Tech:** Rust + Slint chỉ cho Settings UI; native tray/menu riêng. Theme dark/light/system, Vietnamese/English, keyboard shortcuts/a11y, high-DPI/Retina.

**Menu khay:** `VI/EN`, `Bật/tắt`, `Telex/VNI/VIQR`, `Unicode`, `Profile`, `Cài đặt`, `Tạm dừng`, `Khởi động cùng máy`, `Thoát`.

**Settings tabs:**
1. **Overview:** trạng thái engine, backend active, test field, first-run activation.
2. **Input:** scheme, encoding, spell-check, tone style, restore key.
3. **Shortcuts:** Toggle VI/EN, conflict detection, scoped shortcuts.
4. **Macro:** danh sách từ viết tắt, import/export và per-app scope.
5. **Applications:** app selector/profiles, game mode.
6. **Compatibility:** native backend, optional Windows hook fallback, troubleshooting.
7. **System:** autostart, appearance, update, config backup.
8. **About/Doctor:** version, licenses, redacted diagnostics, compatibility info.

**First-run:** chọn scheme → hướng dẫn thêm Input Source/engine đúng OS → kiểm tra gõ thử → cài hotkey → hoàn thành. Settings UI phải phân biệt `Process running` và `IME actually activated`; không báo hoạt động khi chưa đăng ký/chọn input source.

## 9. Configuration & IPC

```toml
schema_version = 1
[general]
enabled = true
mode = "vi"
scheme = "telex"
encoding = "unicode_nfc"
run_at_login = false

[typing]
tone_style = "modern"
spell_check = true
restore_original_on_escape = true

[macros]
enabled = true

[[profiles]]
match_type = "app_id"
match_value = "com.example.editor"
mode = "en"
macros_enabled = false
```

Lưu trong `%APPDATA%\VietIME\`, `~/Library/Application Support/VietIME/`, `$XDG_CONFIG_HOME/vietime/` (hoặc `~/.config/vietime/`). Atomic write, schema migration, backup, corruption recovery. Settings ↔ engine qua IPC **local same-user** có ACL; không chuyển composition text qua IPC/log.

## 10. Bảo mật và quyền riêng tư

- Không lưu/ghi log/raw trace/send network nội dung người dùng nhập, từ đơn lẻ, clipboard, secure fields; diagnostics phải redact dữ liệu nhạy cảm.
- Không dùng global key interception mặc định; không root/admin ở runtime. OS installer được phép yêu cầu quyền cài package theo chuẩn.
- Engine không cần internet; updater tách process và dùng signed manifest + artifact signature/hash, rollback.
- FFI `catch_unwind`, không unwind qua C ABI, quản lý memory ownership, giới hạn buffer/overflow; TSF callbacks không block, không làm treo host.
- Không bỏ qua security prompts và không bypass UAC/anti-cheat/sandbox. Secure input luôn ưu tiên hạn chế chức năng hơn là can thiệp.
- CI `cargo audit`/dependency audit, SBOM, license review, code signing, security checklist và threat model.

## 11. Hiệu năng (mục tiêu, chưa đo)

| Metric | Target | Cách đo |
|---|---|---|
| Rust core per-key p95 | <1 ms | Criterion release build |
| End-to-end compose p95 | <10 ms | Platform integration trace |
| Idle CPU | <0.5% | OS process profiler trong 5 phút ổn định |
| Engine/headless RSS | <30 MB | Working set/RSS/PSS; UI riêng |
| Startup | <1 s sau logon trên SSD tham chiếu | Timestamp tracing |
| Gõ nhanh | 120–200 WPM không rơi/lặp | Stress typing harness |

Mọi target phải ghi loại máy/OS/cpu/toolkit/backend. Không kết luận “nhanh hơn EVKey” nếu không có benchmark công bằng.

## 12. Test strategy & compatibility matrix

**Unit:** mỗi phím dấu/shape, tone placement, Unicode normalization, uppercase, undo, context reset, macro priority.  
**Golden corpus:** hàng trăm chuỗi Telex/VNI/VIQR, thứ tự dấu khác nhau, tiếng Anh/URL/code, emoji, paste, selection, focus/mode change, secure fields.  
**Property/fuzz:** invariants no panic, NFC idempotence, composition isolation, không xóa text host; stress 1 triệu generated sequences là mục tiêu test.  
**FFI:** ABI tests, panic isolation, cleanup, sanitizer nếu có.  
**E2E:** máy OS thực mới quyết định compatibility, không lấy `cargo test` làm bằng chứng native IME hoạt động.

| Test environment | Apps | Test categories |
|---|---|---|
| Windows 11 x64 | Notepad, Office, Chrome, Edge, VS Code, PowerShell | TSF, focus, autocomplete, elevated limitation |
| macOS arm64/x64 | TextEdit, Safari, Notes, Chrome, VS Code, Terminal | IMK preedit/marked text, switching input source, password |
| GNOME Wayland/IBus | GTK editor, Firefox, Chrome/Electron, Terminal | D-Bus, compositor, preedit/commit |
| KDE Wayland/Fcitx5 | Kate, Firefox, VS Code, Konsole | Addon ABI, compositor/text-input |
| X11/IBus-Fcitx5 | GTK, Qt, terminal | Unicode, focus, shortcuts |

Record test state as `PASS`, `LIMITED`, `FAIL`, `NOT_TESTED`; lưu phiên OS, app, toolkit, backend, steps và logs **không có nội dung gõ**.

### Golden fixture samples

```yaml
- id: TX001
  scheme: telex
  input: tieengs
  expected: tiếng
  output_form: NFC
- id: TX002
  scheme: telex
  input: ddawng
  expected: đăng
- id: VN001
  scheme: vni
  input: tie61ng
  expected: tiếng
- id: VQ001
  scheme: viqr
  input: a^
  expected: â
```

## 13. CI, Packaging, Release

- GitHub Actions matrix: `fmt`, `clippy -D warnings`, unit/integration for core, audit/license/SBOM, build Windows/macOS/Linux. Native E2E trên runner/sandbox tương ứng; self-hosted khi cần GUI session.
- **Windows:** MSI/WiX hoặc NSIS + TSF DLL/COM profile registration, code signing; cài/gỡ/update sạch; kiểm tra x64 trước ARM64.
- **macOS:** input method `.app`/`.pkg`, signing/notarization, first-run guide; build Intel/Apple Silicon; hướng dẫn chọn input source. Không phát hành app UI thông thường rồi coi là IME đã được cài.
- **Linux:** package `.deb`/`.rpm`, tách `vietime-ibus`, `vietime-fcitx5`, `vietime-settings`; đăng ký component/addon; test upgrade/uninstall không phá config người dùng. AppImage chỉ phù hợp GUI helper, không thay thế package IME native.
- Signed artifacts + checksums, GitHub Release stable/beta, changelog, rollback, docs cho từng OS.

## 14. Roadmap, milestones, Definition of Done

**P0 — Discovery/ADR:** nghiên cứu legal/OS, tạo `ADR-001-core.md`, `ADR-002-windows-tsf.md`, `ADR-003-macos-imk.md`, `ADR-004-linux-ime.md`; làm 4 POC gõ một ký tự trong Windows TSF, macOS IMK, Linux IBus, Fcitx5. Nếu POC chưa chạy **không claim support**.

**P1 — Core engine:** workspace Rust, `ime-core`, `ime-unicode`, `ime-telex`, `ime-vni`, history/state machine, golden tests, minimal CLI, C ABI.

**P2 — Integration:** TSF/IMK/IBus/Fcitx5 bridge, composition lifecycle, per-context reset, capability detection. Gõ tiếng Việt thật trong Notepad/TextEdit/GTK/Qt app.

**P3 — UX:** Slint Settings, tray/menu, hotkeys, profiles, macro, game mode, first-run onboarding, doctor.

**P4 — Hardening 1.0:** security audit, e2e matrix, signing, installer/uninstaller, docs, native compatibility; không ship stable đến khi P0 đạt ít nhất một cấu hình **được công bố** mỗi OS.

**P5 — 1.1:** legacy encoding conversion, signed updater, optional Windows hook fallback, extra macro import, more app fixes.

### GitHub issue seeds

| Issue | Deliverable | Depends on |
|---|---|---|
| CORE-001 | Event protocol + capability contract | — |
| CORE-002 | Unicode + grapheme + tests | CORE-001 |
| CORE-003 | Telex grammar/rules | CORE-002 |
| CORE-004 | VNI grammar/rules | CORE-002 |
| CORE-005 | Composition state machine | CORE-001 |
| WIN-001 | Windows TSF native POC | CORE-001 |
| MAC-001 | macOS IMK native POC | CORE-001 |
| LIN-001 | IBus native POC | CORE-001 |
| LIN-002 | Fcitx5 native POC | CORE-001 |
| UI-001 | Settings + tray + activation | Native POCs |
| SEC-001 | Privacy + FFI security tests | All adapters |
| QA-001 | Actual compatibility matrix | All adapters |
| REL-001 | Installers + release | QA-001 |

## 15. Prompt triển khai cho Codex

```text
Act as the lead Rust systems engineer of VietIME, a clean-room Vietnamese
input method for Windows, macOS and Linux. Read this SPEC in full and create
an implementation plan before changing code. Do not copy EVKey's binaries,
branding, or unlicensed implementation.

Non-negotiable:
1. Implement one deterministic, testable Rust core for Telex, VNI, Unicode
   normalization, composition state and per-input-context isolation.
2. Use OS-native IME integration: TSF on Windows; InputMethodKit on macOS;
   IBus and Fcitx5 on Linux. Thin C++/ObjC bridges are permitted.
3. Do not substitute a global keyboard hook for real IME support.
4. Do not record keystrokes, send typed text to network, access clipboard by
   default, bypass protected processes, or require admin/root at runtime.
5. Handle preedit, commit, focus, caret movement and backspace without
   deleting external text or double-committing text.
6. Make golden tests and a CLI engine simulator before designing the UI.
7. Prove each native POC on its target OS. Never claim tests were run if they
   were not; mark unresolved OS-specific work as blocked with evidence.
8. Split tasks into reviewable, independently buildable commits/PRs. Add
   exact file paths, build instructions, evidence, constraints, risks.

Execution order: ADR → Rust workspace/core → fixtures → native POCs → full
adapters → Slint settings/tray → macros/profiles → CI/security → installers.

For each iteration deliver: changed files, implementation notes, commands,
unit/integration/e2e results, OS limitations, next actionable backlog items.
```

## 16. Chấp nhận cuối cùng và tiêu chí không được bỏ qua

- **Windows:** TSF đã đăng ký, chọn được và gõ từ app bên thứ ba; failure cases không làm crash ứng dụng host.
- **macOS:** được macOS nhận thành input source, gõ được TextEdit; không dựa vào Accessibility hook mặc định.
- **Linux:** có gói IBus/Fcitx5 hoạt động ở session được công bố; Wayland caveats được ghi rõ.
- **Core:** cùng key sequence + scheme + profile → cùng Unicode output trên cả ba OS.
- **Privacy:** không có log/telemetry chứa raw text; không phát tán dữ liệu nhạy cảm qua crash report.
- **Installer:** máy sạch cài mới, nâng cấp, gỡ và khôi phục input method cũ đúng cách.
- **Bảo hành phát biểu:** không quảng cáo tương thích tất cả game/Wayland/password field khi không có kết quả test.

## 17. Tài liệu nguồn

- EVKey: https://github.com/lamquangminh/EVKey
- EVKey feature documentation: https://github.com/lamquangminh/EVKey/blob/master/docs/_includes/content.html
- EVKey updater source: https://github.com/lamquangminh/EVKey/tree/master/evkau
- Microsoft TSF: https://learn.microsoft.com/en-us/windows/win32/tsf/text-services-framework
- Microsoft IME requirements: https://learn.microsoft.com/en-us/windows/apps/develop/input/input-method-editor-requirements
- Microsoft SendInput: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- Apple InputMethodKit: https://developer.apple.com/documentation/inputmethodkit
- Apple IMKServerInput: https://developer.apple.com/documentation/inputmethodkit/imkserverinput
- IBus: https://github.com/ibus/ibus
- Fcitx5 addon development: https://fcitx-im.org/wiki/Develop_an_simple_input_method/en
- Fcitx5 on Wayland: https://fcitx-im.org/wiki/Using_Fcitx_5_on_Wayland/en
- Unicode normalization UAX #15: https://www.unicode.org/reports/tr15/

**Định hướng:** Rust core dùng chung + native adapter riêng theo OS + UI cấu hình nhẹ. Không xây app hook phím toàn cầu và gọi đó là IME đa nền tảng.
