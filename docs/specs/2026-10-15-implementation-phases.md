# 2026-10-15 — Phân phase triển khai pKey

## Context / Why
Người dùng yêu cầu chia phase triển khai. §26 của `SPEC_pKey_Full_Build.md` chỉ có bảng roadmap tóm tắt, thiếu work package, ước lượng, track song song và gate đo được.

## Decision
- Tạo `docs/IMPLEMENTATION_PHASES.md` làm nguồn chính cho kế hoạch. §26 của spec chỉ giữ bảng tóm tắt và trỏ sang file này.
- Tổ chức lại roadmap thành 7 phase: P0 Foundation, P1 Core, **P2 Windows vertical slice**, P3 macOS + Linux, P4 UX, P5 Hardening 1.0, P6 1.1.
- Làm Windows trước (vertical slice + Alpha nội bộ) thay vì làm cả 4 adapter cùng lúc. Lý do: máy phát triển chính là Windows và TSF là rủi ro lớn nhất.
- Phương án đã cân nhắc: tích hợp đồng thời 4 backend như roadmap cũ. Không chọn vì phát hiện lỗi kiến trúc muộn và không có bản dogfood sớm.

## Design
- Giả định 3 track (A Core, B Windows, C macOS/Linux), ước lượng theo person-day, đường găng, 7 gate G0–G6, lịch ví dụ bắt đầu 2026-10-19 có 1 tuần dự phòng Tết.
- Spec §26: thay bảng phase cũ; cột Phase của backlog được đánh lại (WIN-002/003, CORE-012, IPC-001 → P2; MAC-002, LIN-003/004 → P3; UI/DIAG/CORE-013 → P4; SEC/QA/REL/DOC → P5).

## API / Schema changes
Không có.

## How to test
Review: mọi issue ID trong IMPLEMENTATION_PHASES.md có trong spec §26.1 hoặc là work package con (hậu tố a/b/c, P0-xx, QA-xx, UI-000, REL-000, CORE-014, PERF-001, LIN-005, ARM-001). Phụ thuộc giữa các work package không có vòng lặp.
