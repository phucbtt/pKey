# 2026-10-16: TSF `Deactivate` rò tham chiếu và chốt chặn `test-hooks`

Nguồn gốc: kiểm toán bảo mật (hai lượt, chỉ đọc mã, báo cáo ở ngoài repo). Không có lỗ hổng được xác nhận; hai mục dưới đây là hardening/lỗi chức năng được hunter tìm ra và đã được sửa.

## 1. TSF: `Deactivate` không giải phóng service (lỗi chức năng)

**Triệu chứng.** Sau khi host gọi `Deactivate`, `TextService` không bao giờ được giải phóng: `LIVE_OBJECTS` không về 0, `DllCanUnloadNow` luôn trả `S_FALSE`, nên `pkey_tsf.dll` bị ghim trong mọi tiến trình host đã từng kích hoạt bàn phím (đến khi tiến trình thoát).

**Nguyên nhân gốc.** `Shared` giữ hai tham chiếu mạnh tới chính `TextService` (`composition_sink`, `edit_sink`) để mã chạy ngoài callback dùng được (chú thích ở `Shared` nói `Deactivate` sẽ phá vòng tham chiếu này, nhưng nó không làm). Ngoài ra:
- `Deactivate` xả bản đồ `contexts` mà không gọi `unadvise_edit_sink`, nên `ITfTextEditSink` đã đăng ký trên từng `ITfContext` của host vẫn giữ tham chiếu tới service.
- Nhánh lỗi của `activate` (ví dụ `AdviseKeyEventSink` hoặc `CoCreateInstance` thất bại) để lại cookie thread-sink, vì `thread_mgr` chỉ được lưu khi mọi bước thành công, và TSF không gọi `Deactivate` cho lần kích hoạt đã trả lỗi.

Đo được bằng test: 6 tham chiếu khi đang chạy, còn 5 sau `Deactivate` (3 thừa) trước khi sửa; còn 2 (chỉ phần test giữ) sau khi sửa.

**Thay đổi.** [platform/windows-tsf/src/com/service.rs](../../platform/windows-tsf/src/com/service.rs):
- `Shared::teardown()` mới, dùng chung cho `Deactivate` và nhánh lỗi của `activate`: bỏ đăng ký key sink và thread sink, `unadvise_edit_sink` cho từng context trước `destroy_context`, xóa `secure_cache`, `pending`, `fed`, và `take()` hai sink để phá vòng tham chiếu. Phiên chỉnh sửa đã xếp hàng giữ bản sao riêng nên không bị ảnh hưởng.
- `activate` tách thành `advise_all`; `thread_mgr` được lưu ngay khi thread sink đã đăng ký; `activate` gọi `teardown` nếu `advise_all` lỗi.
- `forget_context` lấy mục ra khỏi `contexts` trước khi gọi COM (không giữ `borrow_mut()` qua `Unadvise`, `destroy_context` và việc thả tham chiếu host).
- `OurEdit` (RAII) thay cho `in_our_edit.set(true/false)` ở hai chỗ: nếu `guard` bắt panic giữa phiên, cờ không bị kẹt ở `true` (cờ kẹt làm di chuyển caret không còn kết thúc composition).

**Test.** `deactivating_releases_every_reference_the_service_took` trong [platform/windows-tsf/tests/composition.rs](../../platform/windows-tsf/tests/composition.rs): đếm tham chiếu COM của service bằng `AddRef`/`Release` qua bảng `IUnknown`, và khẳng định chỉ còn 2 tham chiếu do chính test giữ sau `Deactivate`. Đã xác nhận test thất bại trên mã cũ ("3 reference(s) còn lại") và đạt trên mã mới. Toàn bộ `cargo test --workspace`, `clippy -D warnings` và `fmt --check` đạt.

**Còn mở.** `OnUninitDocumentMgr` chỉ quên context trên cùng của tài liệu (`GetTop`); các context khác trong ngăn xếp của tài liệu ở lại `contexts`/`secure_cache` đến `Deactivate` (rò rỉ có giới hạn theo từng tài liệu). Không đổi trong lần này.

## 2. Chốt biên dịch cho `test-hooks` (hardening)

**Rủi ro.** Tính năng `test-hooks` của `pkey-app` làm hook bàn phím chấp nhận phím giả mang `dwExtraInfo == TEST_MAGIC`, tức là vô hiệu hóa bộ lọc `LLKHF_INJECTED`; của `pkey-tsf` cho phép `ActivateEx` bỏ qua `AdviseKeyEventSink` bị từ chối. Mọi script phát hành hiện không bật nó, nhưng không có gì ngăn một bản build có cờ này bị phát hành nhầm.

**Thay đổi.** `compile_error!` khi `feature = "test-hooks"` kết hợp với `not(debug_assertions)`, ở [platform/windows-app/src/lib.rs](../../platform/windows-app/src/lib.rs) và [platform/windows-tsf/src/lib.rs](../../platform/windows-tsf/src/lib.rs). Mọi profile phát hành (`release`, `dist`, installer) đều tắt `debug_assertions`, nên bị chặn. Các test e2e và test TSF dùng bản debug nên không đổi (đã kiểm: `cargo check --features test-hooks` ở debug qua, ở `--release` báo lỗi, cả hai crate; build `--release` không có cờ vẫn qua).

**Giới hạn.** Chạy e2e ở chế độ release cần thêm `RUSTFLAGS="-C debug-assertions=on"`. Chốt này chỉ chặn bằng cấu hình biên dịch, không thay thế việc kiểm tra mã nhị phân đầu ra.
