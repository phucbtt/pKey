# 2026-10-15 — Hoàn thiện lõi P1: VIQR, config, macro, runtime, C ABI, kiểm thử vét cạn

**Phase:** P1 (CORE-005…CORE-014, CLI-001) và P0-03 (CI). Căn cứ: `SPEC_pKey_Full_Build.md` §8–§13, §16, §22; `IMPLEMENTATION_PHASES.md` §3.

## Những gì đã thêm

| Crate / file | Nội dung | Spec |
|---|---|---|
| `ime-core/src/composition.rs` | VIQR: `( ^ +` cho dấu mũ/móc, `' \` ? ~ .` cho thanh, `dd` → đ, `\` thoát ký tự kế tiếp. Dấu chỉ nhận khi đang trong một từ | §9.3 |
| `ime-core/src/hotkey.rs` | `Hotkey` (parse/in) + `HotkeyMatcher` (modifier-only bắn khi nhả modifier cuối, không bắn nếu có phím khác xen giữa) | §16.3 |
| `ime-config` | Schema v1 đầy đủ, validate gom **mọi** lỗi kèm đường dẫn khóa (`general.scheme`), cảnh báo khóa lạ, khung migrate `vN→vN+1`, ghi lại bằng `toml_edit` (giữ comment + khóa lạ), lưu atomic + 5 backup xoay vòng, file hỏng đổi tên `config.toml.corrupt-<ts>`, `Paths` theo OS, resolve profile + game mode | §12, §16 |
| `ime-macro` | `MacroTable` (khớp raw trước, rồi chuỗi hiển thị; `case_follow`; whole-word / hậu tố; profile > global > priority), validate (giới hạn, trùng trigger/id), `macros.json`, import/export CSV có preview + báo lỗi theo dòng, merge, backup | §11 |
| `ime-core/src/engine.rs` | Mở rộng macro khi commit ở Space/Enter/Tab/dấu câu; `set_macros`, `set_context_macros`, **`set_context_config`** (tùy chọn gõ theo profile cho từng context) | §11, §12 |
| `ime-runtime` | Lớp dán dùng chung cho mọi adapter: engine + config + macro + profile theo app + phím tắt chuyển VI/EN + reload khi file đổi (`poll_reload`, file lỗi thì giữ cấu hình đang chạy) | §12, §15, §16 |
| `ime-ffi` + `include/pkey.h` | C ABI theo §13: mọi hàm bọc `catch_unwind`, panic ⇒ `VT_ERR_PANIC` + poison (fail-open), kiểm `struct_size`, kiểm UTF-8, kiểm enum từ C trước khi đổi sang enum Rust | §13 |
| `tests/ffi/harness.c` + `crates/ime-ffi/tests/c_harness.rs` | Harness C biên dịch bằng compiler hệ thống (qua `cc`), link với cdylib, chạy; `PKEY_C_SANITIZE=address,undefined` bật sanitizer | §13.2 |
| `ime-cli` | `pkey-cli config default \| validate \| migrate [--write]`, `--scheme viqr` | §18 |
| `.github/workflows/ci.yml`, `deny.toml` | fmt + clippy + test trên 3 OS, harness C bắt buộc, harness dưới ASan/UBSan trên Linux, `cargo deny` | §23 |

## Lỗi thật phát hiện nhờ test mới (nguyên nhân gốc)

1. **Chữ số kích hoạt macro.** `bitflags` `contains(empty)` luôn đúng, nên ký tự không phải dấu phân cách (chữ số) vẫn mở rộng macro. Sửa: `commit_at` loại `delimiter.is_empty()` rõ ràng. (`engine::tests::macro_ignores_disabled_delimiters_and_non_delimiters`)
2. **`uơ` không gõ được** (`thuở`, `huơ`): `uow`/`uo7`/`uo+` luôn thành `ươ`; `ươ` đứng cuối từ không hợp lệ ⇒ từ bị khôi phục về phím thô. Sửa: `validity` coi `ươ` hở như `uơ`, `render` hiển thị `uơ`; thêm phụ âm cuối thì lại thành `ươ` (`thương`). Chỉ là cách *hiển thị*, `letters` giữ nguyên.
3. **`oeo` không gõ được** (`khoèo`, `ngoẹo`): `classify` coi `ôe` là Prefix hợp lệ (vì so khớp sau khi bỏ dấu), nên phím `o` thứ hai gán dấu mũ cho `o` đầu rồi cả từ bị khôi phục. Sửa: so khớp từng vị trí nguyên âm — nguyên âm đã gõ phải bằng nguyên âm trong bảng, hoặc còn **chưa có dấu** và bảng có dạng có dấu. `ôe` giờ là Invalid nên phím bị từ chối đúng.

Cả hai lỗi 2 và 3 chỉ lộ ra khi sinh toàn bộ âm tiết; 164 ca golden viết tay không bắt được.

## Kiểm thử

- `cargo test --workspace`: toàn bộ pass (xem số liệu ở dưới).
- **Vét cạn** (`ime-testing/tests/syllables.rs`): mọi âm tiết hợp lệ từ `onsets() × rhymes() × thanh` (~19 000 mỗi kiểu gõ) được mã hóa thành phím Telex/VNI/VIQR bằng bộ mã hóa viết độc lập với engine, gõ vào FakeHost, kết quả phải bằng chính âm tiết đó; thêm biến thể viết hoa chữ đầu, VIẾT HOA TOÀN BỘ, và kiểu đặt dấu `traditional`. Thay cho yêu cầu "≥ 300 golden sinh tự động" của G1.
- Golden viết tay: 164 ca (Telex, VNI, VIQR + ca hồi quy cho lỗi 2 và 3).
- Proptest (`invariants.rs`): I1 (`would_consume == consumed`), I3 (preedit ≤ 32 grapheme), không panic, FakeHost không vi phạm hợp đồng, mọi chuỗi chỉ gồm phụ âm không phím biến đổi ⇒ output = input, các context độc lập.
- Latency (`latency.rs`): release p50 ≈ 0,4 µs, p95 ≈ 1,1 µs, p99 ≈ 1,3–1,9 µs mỗi phím (ngân sách 1 ms), trên máy dev Windows 11, chưa ghi cấu hình CPU chi tiết.
- C harness đã chạy thật bằng MSVC trên Windows (không bị skip).

## Chưa làm / hạn chế còn mở

- **Fuzz** (4 target `cargo-fuzz`, ≥ 1 giờ mỗi target — gate G1) chưa làm: cần nightly + `cargo-fuzz`. Hiện thay bằng proptest (1 500 ca × 5 thuộc tính) và 9 000 kịch bản ngẫu nhiên cố định seed. **Gate G1 chưa đạt mục này.**
- **ASan/UBSan** cho harness C mới chỉ được cấu hình trong CI Linux, chưa chạy ở máy này (MSVC ASan chưa thử). `cargo deny` và workflow CI chưa chạy lần nào (chưa có `cargo-deny`, chưa đẩy lên GitHub).
- `ime-config` còn thiếu so với spec: `mode_scope = per_app` chưa lưu `state.json`; mã hóa `tcvn3`/`vni_windows` bị từ chối (đúng với "1.1"); các tùy chọn 1.1 (`quick_telex`, `auto_capitalize`…) chỉ được lưu, engine chưa dùng.
- Ghi lại config chỉ giữ comment/khóa lạ ở các bảng đã biết và ở cấp gốc; bảng `[[profiles]]` được viết lại hoàn toàn nên comment/khóa lạ **bên trong profile** sẽ mất.
- Macro: trigger chỉ khớp được trong phạm vi một từ do engine soạn (chữ cái, và chữ số với VNI); trigger chứa dấu câu sẽ không bao giờ khớp. Trigger hậu tố (`whole_word = false`) khớp trên chuỗi hiển thị, không trên chuỗi thô.
- C ABI: không có `debug_assert` kiểm tra thread như §13.2 nêu (có thể gây dương tính giả khi host đổi thread hợp lệ); chỉ ghi rõ trong header là mỗi engine một thread.
- `vt_*` header viết tay (chưa có `cbindgen`); không có kiểm tra tự động header khớp code Rust ngoài việc harness C biên dịch được với nó.
- `HotkeyMatcher` gộp modifier trái/phải; Alt phải coi là Alt.
- Cần `PKEY_CONFIG_DIR` để thử với thư mục cấu hình riêng.
