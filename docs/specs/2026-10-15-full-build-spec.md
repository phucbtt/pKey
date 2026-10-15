# 2026-10-15 — Viết đặc tả xây dựng đầy đủ VietIME

## Context / Why
`docs/SPEC_VietIME_Rust_CrossPlatform.md` (v1.0) mới ở mức đề xuất định hướng; chưa đủ chi tiết để bắt tay triển khai (thiếu kiểu dữ liệu, luật ngữ âm, C ABI, chi tiết adapter, schema config, kế hoạch kiểm thử/đóng gói).

## Decision
Tạo tài liệu mới `docs/SPEC_VietIME_Full_Build.md` (v2.0) mở rộng spec gốc thay vì sửa trực tiếp spec gốc — spec gốc giữ vai trò định hướng, tài liệu mới là đặc tả triển khai. Các giả định chưa kiểm chứng được đánh dấu `[VERIFY]` để POC xác nhận.

## Design
Nội dung bổ sung chính: yêu cầu chức năng có tiêu chí chấp nhận (F001–F021), NFR; process model theo OS; trách nhiệm/phụ thuộc từng crate; kiểu dữ liệu & API `Engine`; bảng state machine; mô hình âm tiết và thuật toán đặt dấu; bảng luật Telex/VNI/VIQR + định dạng mapping; C ABI (`vietime.h`); chi tiết adapter TSF/IMK/IBus/Fcitx5; state bus & IPC; schema config đầy đủ; UI/onboarding; CLI; threat model; logging redaction; chiến lược test; CI/CD; đóng gói; updater; roadmap + backlog; rủi ro; câu hỏi mở; checklist 1.0.

## API / Schema changes
Chưa có code. Tài liệu định nghĩa các interface *đề xuất*: Rust `Engine` API (§7), C ABI v1 (§13), config schema v1 (§16.2), macro schema v1 (§11.1), IPC message v1 (§15.3).

## How to test
Review tài liệu; các mục `[VERIFY]` được xác nhận trong POC P0 (WIN-001, MAC-001, LIN-001, LIN-002) và ADR tương ứng.
