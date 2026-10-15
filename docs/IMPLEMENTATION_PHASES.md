# pKey — Kế hoạch phân phase triển khai

| Thuộc tính | Giá trị |
|---|---|
| Ngày | 2026-10-15 |
| Căn cứ | [SPEC_pKey_Full_Build.md](SPEC_pKey_Full_Build.md) v2.0 (tham chiếu `§` là mục trong spec đó) |
| Giả định đội | 3 kỹ sư: **A** = Core/Rust, **B** = Windows, **C** = macOS + Linux. Từ P4, A và B chuyển sang UX |
| Đơn vị ước lượng | pd = person-day (ngày công). Ước lượng có sai số ±30%, hiệu chỉnh lại sau Gate G0 |
| Thứ tự nền tảng | **Windows trước** (vertical slice hoàn chỉnh) rồi mới đến macOS/Linux. Lý do: máy phát triển chính là Windows 11, TSF rủi ro cao nhất nên cần bằng chứng sớm. Có thể đổi nếu sản phẩm ưu tiên nền tảng khác |

---

## 1. Tổng quan

```text
Tuần:   1   2   3   4   5   6   7   8   9  10  11  12  13  14  15  16  17  18  19  20  21  22  23
P0  [Foundation + POC ]
P1      [========== Core engine ==========]
P2                  [===== Windows vertical slice =====]  ★ Alpha-Win (W12)
P3                              [====== macOS + Linux adapters ======]
P4                                      [=========== UX & tính năng ===========]
--                                                              [Tết]
P5                                                                  [==== Hardening ====] ★ Beta W19 · RC W22 · GA W23
P6  (1.1)                                                                                  W24 → W31
```

| Phase | Mục tiêu một câu | Tuần | Track | Gate |
|---|---|---|---|---|
| **P0** Foundation & Discovery | Có repo/CI chạy, ADR đã chốt, 4 POC chứng minh gõ được trên từng backend | W1–W4 | A, B, C | G0 |
| **P1** Core engine | Lõi Rust gõ đúng Telex/VNI qua CLI và FakeHost, có C ABI | W2–W9 | A | G1 |
| **P2** Windows vertical slice | Gõ tiếng Việt thật hằng ngày trên Windows (TSF + config + toggle) | W5–W12 | B (+A từ W8) | G2 → **Alpha-Win** |
| **P3** macOS + Linux adapters | Gõ thật trên macOS (IMK), IBus, Fcitx5 | W8–W16 | C | G3 |
| **P4** UX & tính năng | Settings, tray/menu, onboarding, macro, profile, VIQR, doctor | W10–W18 | A, B | G4 → **Beta** |
| **P5** Hardening & Release 1.0 | Bảo mật, ma trận tương thích, installer ký, tài liệu | W18–W23 | A, B, C | G5 → **1.0 GA** |
| **P6** Release 1.1 | Bảng mã cũ, updater, hook fallback, Quick Telex | W24–W31 | tùy nguồn lực | G6 |

Đường găng (critical path): **P0 WIN-001 → P1 CORE-006 → P2 WIN-002 → P4 UI-001 → P5 REL-001**. Trễ ở bất kỳ mắt xích nào làm trễ GA.

Nguyên tắc chung cho mọi phase:
- Mỗi PR nhỏ, build độc lập, CI xanh, kèm change spec trong `docs/specs/`.
- Không đánh dấu PASS khi chưa chạy trên máy thật. Việc bị chặn ghi **BLOCKED** kèm bằng chứng.
- Gate không đạt thì không bước sang phase phụ thuộc. Hoặc thu hẹp phạm vi (ghi ADR), hoặc kéo dài phase.

---

## 2. P0 — Foundation & Discovery (W1–W4)

**Mục tiêu:** loại bỏ rủi ro kỹ thuật lớn nhất (native IME từ Rust) trước khi đầu tư vào lõi và UI.

### Work packages
| ID | Việc | Track | pd | Spec |
|---|---|---|---|---|
| P0-01 | Chốt license dự án, tên/bundle id/publisher, clean-room log (Q1) | A | 2 | §0.2, §28 |
| P0-02 | Khởi tạo workspace: `Cargo.toml`, `rust-toolchain.toml`, `deny.toml`, khung crate rỗng, `docs/` | A | 2 | §6 |
| P0-03 | CI tối thiểu: fmt, clippy, test trên 3 OS; `cargo deny` | A | 2 | §23 |
| P0-04 | ADR-001 Core & event contract; ADR-005 State bus/IPC (bản nháp) | A | 3 | §7, §15 |
| P0-05 | ADR-002 + **WIN-001**: TIP Rust (crate `windows`), đăng ký, StartComposition, gõ `aa`→`â` cố định; build x64 + x86 | B | 12 | §14.1 |
| P0-06 | ADR-003 + **MAC-001**: bundle IMK + shim Swift/ObjC gọi một hàm Rust `extern "C"`, `setMarkedText`/`insertText` | C | 6 | §14.3 |
| P0-07 | ADR-004 + **LIN-001**: engine IBus (thử C shim, so sánh nhanh với zbus) | C | 4 | §14.4 |
| P0-08 | **LIN-002**: addon Fcitx5 tối thiểu gọi Rust staticlib | C | 3 | §14.5 |
| P0-09 | ADR-006 Settings UI (Slint) + tray stack: dựng thử 1 cửa sổ trên 3 OS | A | 2 | §17 |
| P0-10 | Lập ngân sách chứng chỉ ký code (Authenticode, Apple Developer ID) | A | 1 | §24 |

### Deliverables
- Repo build được trên 3 OS, CI xanh.
- `docs/adr/ADR-001…006.md`.
- Thư mục `platform/*/poc` cùng video/screenshot gõ thử và log build.

### Gate G0 (cuối W4)
- [ ] Mỗi backend trong 4 backend (TSF, IMK, IBus, Fcitx5) gõ được ít nhất một ký tự biến đổi bằng preedit/commit, **hoặc** được ghi BLOCKED kèm nguyên nhân và phương án thay thế.
- [ ] TIP x86 nạp được trong một ứng dụng 32-bit [VERIFY].
- [ ] ADR-002/003/004 đã chọn phương án (Rust thuần hay shim).
- [ ] Ước lượng P1–P5 đã hiệu chỉnh lại theo thực tế POC.

**Rủi ro chính:** TSF từ Rust tốn thời gian hơn dự kiến. Phương án: cho phép shim C++ mỏng (đã cho phép trong ADR-002) và gia hạn tối đa 1 tuần.

---

## 3. P1 — Core engine (W2–W9)

**Mục tiêu:** một lõi Rust pure, deterministic, đạt toàn bộ golden test Telex/VNI. Đây là nền cho mọi adapter.
Chạy song song với các POC từ W2 vì lõi không phụ thuộc OS.

### Work packages (theo thứ tự phụ thuộc)
| ID | Việc | pd | Phụ thuộc | Spec |
|---|---|---|---|---|
| CORE-001 | Kiểu dữ liệu công khai, `KeyInput`, `EngineAction`, `Capabilities`, `Output` | 3 | P0-04 | §7.1 |
| CORE-011a | `ime-testing`: FakeHost, golden runner YAML (làm sớm để TDD) | 4 | CORE-001 | §22.2 |
| CORE-002 | `ime-unicode`: bảng 134 ký tự, render NFC/NFD, `Caret` 4 đơn vị | 4 | CORE-001 | §10 |
| CORE-003 | `rhymes.toml` + parser âm tiết + validity + thuật toán đặt dấu (+ `IME_RULES.md`) | 8 | CORE-002 | §8 |
| CORE-006 | State machine, `Composition`, contexts, backspace 2 mode, auto restore, `would_consume` | 7 | CORE-003 | §7.3–7.7 |
| CORE-004 | Telex (mapping TOML + executor chung) | 5 | CORE-006 | §9.1, §9.4 |
| CORE-005 | VNI | 3 | CORE-004 | §9.2 |
| CORE-007 | Hotkey parser + phát hiện modifier-only | 2 | CORE-001 | §16.3 |
| CORE-008 | `ime-config`: schema v1, validate, migrate framework, atomic save, `toml_edit` round-trip | 5 | — | §16 |
| CORE-009 | `ime-macro` (lõi, chưa có UI) | 3 | CORE-002 | §11 |
| CORE-010 | `ime-ffi` + cbindgen + C harness + ASan/UBSan | 5 | CORE-006 | §13 |
| CORE-011b | proptest invariants I1–I4, 4 target fuzz, bench Criterion | 4 | CORE-006 | §22.3, §21 |
| CLI-001 | `pkey-cli simulate`, `golden run`, `config validate/migrate` | 3 | CORE-011a | §18 |

Tổng khoảng 56 pd (track A, có hỗ trợ từ B/C khi xong POC).

**Mốc trung gian:**
- **W5** — `pkey-cli simulate --scheme telex "tieengs vieejt"` ra kết quả đúng (Telex cơ bản).
- **W7** — C ABI sẵn sàng để C dùng cho IMK/Fcitx5.
- **W9** — Gate G1.

### Gate G1 (cuối W9)
- [ ] Golden ≥ 300 case (Telex + VNI, sinh tự động từ `rhymes.toml`) pass trên CI cả 3 OS.
- [ ] Bộ test đặt dấu §8.3 pass ở cả `modern` và `traditional`.
- [ ] Fuzz tích lũy ≥ 1 giờ mỗi target, không crash; proptest I1–I4 pass.
- [ ] Bench `process_key_telex` p95 < 1 ms (ghi cấu hình máy).
- [ ] C harness chạy sạch dưới ASan/UBSan.
- [ ] `cargo deny` sạch; crate lõi `#![forbid(unsafe_code)]`.

---

## 4. P2 — Windows vertical slice (W5–W12)

**Mục tiêu:** bản Alpha dùng được hằng ngày trên Windows. Đây là bằng chứng end-to-end đầu tiên cho kiến trúc.

### Work packages
| ID | Việc | Track | pd | Phụ thuộc | Spec |
|---|---|---|---|---|---|
| WIN-002a | Khung TIP sản phẩm từ POC: COM factory, `ActivateEx`, key sink, thread mgr sink, `catch_unwind` mọi callback | B | 6 | G0 | §14.1 |
| WIN-002b | Composition đầy đủ: edit session, display attribute, commit/cancel, `OnCompositionTerminated`, text edit sink (caret move) | B | 8 | WIN-002a, CORE-006 | §14.1 |
| WIN-002c | Tích hợp `ime-core` trực tiếp; map VK/`ToUnicodeEx` → `KeyInput`; `OnTestKeyDown` dùng `would_consume` | B | 4 | CORE-004 | §14.1, Phụ lục C |
| WIN-002d | Secure field (InputScope), app identity (`exe_name`, AUMID) | B | 3 | WIN-002c | §12.2 |
| WIN-002e | Toggle VI/EN: hotkey modifier-only + global compartment + lang bar indicator | B | 5 | CORE-007 | §15.2 |
| WIN-003 | Config snapshot + ACL `ALL APPLICATION PACKAGES` + reload theo generation counter; thử với app UWP | B | 5 | CORE-008 | §14.1 |
| CORE-012 | Profile resolve + game mode (lõi) | A | 3 | CORE-008 | §12 |
| IPC-001a | `ime-ipc` v1 (named pipe) + state bus Windows | A | 5 | ADR-005 | §15 |
| UI-000 | Tray tối thiểu (`pkey-tray.exe`): VI/EN, Telex/VNI, Thoát | A | 3 | IPC-001a | §17.2 |
| REL-000 | Installer MSI dev (chưa ký): đăng ký TIP x64 + x86, gỡ sạch | B | 5 | WIN-002a | §24.1 |
| QA-W1 | Kịch bản E2E §22.5 trên Notepad, Chrome, VS Code, Word | B | 3 | tất cả trên | §22.5 |

Tổng khoảng 50 pd.

### Gate G2 → **Alpha-Win** (cuối W12)
- [ ] Cài bằng MSI trên máy Windows 11 sạch; TIP xuất hiện trong danh sách bàn phím Tiếng Việt.
- [ ] Kịch bản E2E §22.5 PASS trên Notepad, Chrome, VS Code; ứng dụng 32-bit gõ được.
- [ ] Toggle VI/EN hoạt động và hiển thị đúng trên lang bar và tray.
- [ ] Đổi config trong file thì TIP nhận ở lần focus tiếp theo.
- [ ] Soak 2 giờ không crash host.
- [ ] Gỡ cài không để lại đăng ký TIP.

Alpha-Win được phát cho nhóm nội bộ (dogfood) và dùng hằng ngày đến hết dự án để thu lỗi thực tế.

---

## 5. P3 — macOS + Linux adapters (W8–W16)

**Mục tiêu:** đưa lõi đã ổn định lên 3 backend còn lại. Chạy song song với P2 và P4 trên track C.

### Work packages
| ID | Việc | pd | Phụ thuộc | Spec |
|---|---|---|---|---|
| MAC-002a | Controller IMK đầy đủ: `handleEvent`, marked text, `commitComposition`, per-client context | 8 | G0, CORE-010 | §14.3 |
| MAC-002b | Menu IME (VI/EN, scheme), FSEvents reload config, `NSDistributedNotificationCenter` | 4 | MAC-002a | §14.3, §15.2 |
| MAC-002c | Build universal (arm64 + x86_64), script cài dev vào `~/Library/Input Methods` | 2 | MAC-002a | §24.2 |
| LIN-003 | IBus đầy đủ: preedit mode COMMIT, properties VI/EN, content type (password), `focus_in_id`, inotify reload | 8 | G0, CORE-010 | §14.4 |
| LIN-004 | Fcitx5 đầy đủ: `FactoryFor` per-IC, capability flags, client preedit/panel preedit, status | 7 | G0, CORE-010 | §14.5 |
| LIN-005 | Đóng gói dev `.deb`/`.rpm` (chưa phát hành), component XML, addon conf | 4 | LIN-003/004 | §24.3 |
| QA-ML1 | E2E §22.5 trên TextEdit, Safari; gedit/GNOME Text Editor + Firefox (GNOME Wayland/IBus); Kate + Chromium (KDE Wayland/Fcitx5) | 5 | tất cả trên | §22.5 |

Tổng khoảng 38 pd. Thứ tự đề xuất: MAC-002 (W8–W11) → LIN-003 (W11–W13) → LIN-004 (W13–W15) → QA (W15–W16).

### Gate G3 (cuối W16)
- [ ] macOS: input source chọn được; E2E PASS trên TextEdit và Safari, cả Apple Silicon lẫn Intel (hoặc Intel ghi NOT_TESTED nếu không có máy).
- [ ] IBus trên GNOME Wayland (Ubuntu 24.04) và Fcitx5 trên KDE Wayland: E2E PASS trên ít nhất 2 app mỗi backend.
- [ ] Cùng chuỗi phím và cùng config cho ra output giống hệt nhau trên 3 OS (so sánh bằng golden qua FakeHost và kiểm tra tay 20 câu mẫu).
- [ ] Mọi hạn chế phát hiện được đã ghi vào `docs/COMPATIBILITY_MATRIX.md`.

---

## 6. P4 — UX & tính năng (W10–W18)

**Mục tiêu:** đủ tính năng cho người dùng cuối (F006–F012) để phát hành Beta.

### Work packages
| ID | Việc | Track | pd | Phụ thuộc | Spec |
|---|---|---|---|---|---|
| UI-001 | Settings shell (Slint) + i18n vi/en + tab Overview, Input, System | A | 8 | IPC-001a, CORE-008 | §17.3 |
| UI-002a | Tab Shortcuts (ghi phím, phát hiện xung đột với phím OS) | A | 3 | UI-001 | §16.3 |
| UI-002b | Tab Macro + import/export JSON/CSV có preview, backup | B | 6 | UI-001, CORE-009 | §11 |
| UI-002c | Tab Applications: chọn app theo từng OS, override, game mode | B | 5 | UI-001, CORE-012 | §12, §17.3 |
| UI-002d | Tab Compatibility, About (license bên thứ ba bằng `cargo about`) | A | 3 | UI-001 | §17.3 |
| UI-003 | Tray Windows đầy đủ (tạm dừng, profile hiện tại); hoàn thiện menu IMK và indicator IBus/Fcitx5 | B + C | 5 | UI-000 | §17.2 |
| UI-004 | Onboarding: phát hiện trạng thái kích hoạt thật trên 3 OS, gõ thử, chọn hotkey | A + C | 8 | UI-001 | §17.4 |
| IPC-001b | IPC macOS/Linux (Unix socket), `GetStatus` / `Pause` / `DoctorReport` | A | 4 | IPC-001a | §15.3 |
| DIAG-001 | `pkey-cli doctor` (CLI + UI) + test redaction | A | 4 | IPC-001b | §18, §20 |
| CORE-013 | VIQR + golden VIQR | A | 4 | CORE-004 | §9.3 |
| CORE-014 | Tùy chọn F007 còn lại (`telex_brackets`, F019 `allow_foreign_onsets`) + golden bật/tắt | A | 3 | CORE-004 | §3 |
| UX-T | Usability test 5 người (Windows trước, macOS/Linux mỗi nền tảng 1–2 người) | A | 3 | UI-004 | §17 |

Tổng khoảng 56 pd.

### Gate G4 → **Beta** (cuối W19; W17 là tuần dự phòng nghỉ Tết, khoảng đầu tháng 2/2027)
- [ ] Toàn bộ AC của F006–F012 đạt trên Windows; đạt trên macOS/Linux, trừ các mục ghi LIMITED kèm lý do.
- [ ] Onboarding không bao giờ báo "đang hoạt động" khi IME chưa thực sự được chọn (kiểm thử cả 3 OS).
- [ ] Usability test: 5/5 người hoàn tất onboarding không cần trợ giúp, hoặc các lỗi phát hiện đã sửa.
- [ ] Test grep log/doctor không chứa chuỗi gõ thử.
- [ ] Bộ cài Beta có thể chưa ký cho macOS/Linux; Windows ký nếu chứng chỉ đã có.

---

## 7. P5 — Hardening & Release 1.0 (W18–W23)

**Mục tiêu:** đạt toàn bộ checklist §29 và phát hành 1.0.

### Work packages
| ID | Việc | Track | pd | Spec |
|---|---|---|---|---|
| SEC-001 | Threat model hoàn chỉnh, review FFI/COM `unsafe`, kiểm tra không socket mạng, DACL IPC, quyết định Q7 | A | 6 | §19 |
| QA-001 | Ma trận tương thích đầy đủ (§12 spec gốc + §22.5) trên máy thật | B + C | 10 | §22.5 |
| QA-002 | Soak 8 giờ mỗi OS, theo dõi RSS/handle leak | B + C | 3 | §22.6 |
| QA-003 | Mở rộng golden ≥ 500 case; fuzz tích lũy ≥ 24 giờ | A | 3 | §22 |
| REL-001a | Windows: MSI ký Authenticode, nâng cấp (Q4) / gỡ / cài lại trên máy sạch | B | 6 | §24.1 |
| REL-001b | macOS: `.pkg` ký + notarize + staple, uninstaller | C | 4 | §24.2 |
| REL-001c | Linux: `.deb`/`.rpm` chính thức, post-install, kiểm tra nâng cấp không phá config | C | 4 | §24.3 |
| REL-001d | `release.yml`: build, ký, checksums, minisign, SBOM, GitHub Release draft | A | 4 | §23, §24.4 |
| DOC-001 | Hướng dẫn người dùng 3 OS, FAQ, known limitations, `SECURITY.md`, changelog | A | 4 | §19.2 |
| PERF-001 | Đo hiệu năng E2E, ghi `docs/perf/` | B | 2 | §21 |

Tổng khoảng 46 pd.

**Mốc:**
- **W19** — Beta công khai (kênh `beta`), mở issue template "báo từ gõ sai".
- **W22** — Release Candidate: code freeze, chỉ nhận fix lỗi chặn.
- **W23** — **1.0 GA**.

### Gate G5 (trước GA)
- [ ] Checklist §29 của spec đạt 100%.
- [ ] Không còn lỗi mức P0/P1 mở; mọi lỗi P2 đã có ghi chú known issue.
- [ ] Nội dung quảng bá/README khớp đúng `COMPATIBILITY_MATRIX.md`.

---

## 8. P6 — Release 1.1 (W24–W31)

| ID | Việc | pd | Spec |
|---|---|---|---|
| F013 | Bảng mã TCVN3/VNI Windows, đối chiếu 2 nguồn, test round-trip, quyết định Q5 | 6 | §10 |
| F021 | Công cụ chuyển mã văn bản/clipboard có preview | 4 | §3 |
| F015 | Updater ký Ed25519, staged rollout, rollback | 8 | §25 |
| F014 | Windows hook fallback opt-in, loop guard, chặn elevated/secure desktop | 8 | §14.2 |
| F016/F017 | Quick Telex, gõ tắt phụ âm cuối | 4 | §3 |
| F018 | Tự viết hoa đầu câu (chỉ khi có surrounding text) | 3 | §3 |
| ARM-001 | Windows ARM64 TIP (nếu có máy test) | 5 | §14.1 |

**Gate G6:** AC của từng feature đạt; hook fallback qua security review riêng trước khi phát hành.

---

## 9. Lịch ví dụ

Giả định W1 bắt đầu thứ Hai **2026-10-19**. Mốc tính vào thứ Sáu cuối tuần.

| Mốc | Tuần | Ngày (ví dụ) |
|---|---|---|
| Gate G0 — POC xong | W4 | 2026-11-13 |
| Gate G1 — Core xong | W9 | 2026-12-18 |
| **Alpha-Win** (G2) | W12 | 2027-01-08 |
| Gate G3 — macOS/Linux | W16 | 2027-02-05 |
| Dự phòng Tết | W17 | khoảng 2027-02-08 → 02-12 |
| **Beta** (G4) | W19 | 2027-02-26 |
| Release Candidate | W22 | 2027-03-19 |
| **1.0 GA** (G5) | W23 | 2027-03-26 |
| **1.1** (G6) | W31 | 2027-05-21 |

Khi đội nhỏ hơn 3 người: chạy P2 rồi P3 tuần tự thay vì song song, cộng thêm khoảng 8–10 tuần. Với 1 người, ước lượng tổng khoảng 9–12 tháng cho 1.0.

---

## 10. Theo dõi tiến độ

- Mỗi work package là một GitHub issue với ID như trên, gắn label `phase:P0…P6` và `track:A|B|C`.
- Mỗi gate là một GitHub milestone; checklist gate được copy vào mô tả milestone.
- Demo cuối mỗi 2 tuần: chạy kịch bản E2E chuẩn trên backend vừa làm.
- Khi một gate trễ quá 1 tuần: họp re-plan, cập nhật file này và ghi change spec trong `docs/specs/`.
