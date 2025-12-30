<h1 align="center">
  <img src="assets/logo.png" alt="FKey Logo" width="128" height="128"><br>
  FKey
</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Windows-0078D6?logo=windows&logoColor=white" />
  <img src="https://img.shields.io/badge/License-BSD--3--Clause-blue.svg" alt="License: BSD-3-Clause">
</p>

<p align="center">
  <strong>Bộ gõ tiếng Việt miễn phí, nhanh, ổn định cho Windows.</strong><br>
  Cài là dùng. Không quảng cáo. Không thu thập dữ liệu.
</p>

<p align="center">
  <img src="assets/screenshot.png" alt="FKey Screenshot" width="100%">
</p>

---

## 📥 Tải về & Cài đặt

### 📦 Tải thủ công

| Nền tảng | Trạng thái | Tải xuống |
|:--------:|:----------:|:---------:|
| **Windows** | ✅ Sẵn sàng | [📥 Tải FKey.zip](https://github.com/miken90/gonhanh.org/releases/latest) |

> **Lưu ý:** File FKey.zip ~70MB (self-contained, không cần cài .NET Runtime)

### Cài đặt nhanh

1. Tải và giải nén `FKey.zip`
2. Chạy `FKey.exe`
3. App sẽ chạy trong system tray (khay hệ thống)

## ✨ Tính năng

### 🔥 Highlight

- 🔍 **Hỗ trợ mọi app** - Chrome, VS Code, Notion, Terminal, Discord, Slack...
- 🔤 **Auto-restore tiếng Anh** — Gõ `text` `expect` `user` `push` `sort` → tự khôi phục khi nhấn space
- ⎋ **Gõ ESC tự khôi phục** — Gõ `user` → `úẻ` → nhấn **ESC** → `user`
- 🔠 **Tự viết hoa đầu câu** — Gõ `ok.` Space `b` → `B` hoa
- ⚡ **Siêu nhanh** — <1ms latency · ~10MB RAM

### 📋 Đầy đủ

- ⌨️ **Telex & VNI** — Chọn kiểu gõ quen thuộc
- 🎯 **Đặt dấu chuẩn** — Tự động theo quy tắc mới: `hoà`, `khoẻ`, `thuỷ`
- ✂️ **Gõ tắt** — `vn` → `Việt Nam`, `ko` → `không`
- 🚀 **Auto-start** — Tự khởi động cùng Windows
- 🔧 **Phím tắt tùy chỉnh** — Đổi Ctrl+Space thành phím bạn muốn

### 🛡️ Cam kết "Ba Không"

- 🚫 **Không thu phí** — Miễn phí mãi mãi, không bản Pro
- 🚫 **Không quảng cáo** — Không popup, không làm phiền
- 🚫 **Không theo dõi** — Offline 100%, mã nguồn mở

---

## 🔤 Auto-restore tiếng Anh

Khi gõ tiếng Anh bằng Telex, một số chữ cái bị nhận nhầm thành modifier tiếng Việt:
- `s` → sắc, `f` → huyền, `r` → hỏi, `x` → ngã, `j` → nặng
- `w` → dấu móc (ư, ơ)

**FKey tự động khôi phục** khi nhấn **Space** nếu phát hiện pattern tiếng Anh.

### ✅ Các pattern được nhận diện

| Pattern | Ví dụ | Giải thích |
|:--------|:------|:-----------|
| **Modifier + phụ âm** | `text` `next` `test` `expect` | x/s theo sau bởi phụ âm |
| **W đầu + phụ âm** | `window` `water` `write` | W không phải phụ âm đầu tiếng Việt |
| **F đầu** | `file` `fix` `function` | F không tồn tại trong tiếng Việt |

---

## 🔧 Dành cho Developer

### Tech Stack

| Layer | Công nghệ |
|-------|-----------|
| **Core Engine** | Rust 2021 (pure `std`, zero runtime deps) |
| **Windows** | WPF/.NET 8 + SetWindowsHookEx + P/Invoke |
| **Testing** | rstest + serial_test (700+ tests) |
| **CI/CD** | GitHub Actions + auto-versioning |

### Build & Test

```bash
# Build Rust core
cd core
cargo build --release
cargo test

# Build Windows app
cd platforms/windows/GoNhanh
dotnet build -c Release
```

### Known Issues

- **Fast typing race condition**: Gõ quá nhanh có thể gây sai thứ tự ký tự
  - Ví dụ: "hiện" → "hinệ", "không" → "kohng"
  - Đang phát triển fix: async queue architecture

---

## 🙏 Lời cảm ơn

FKey được fork từ dự án **[Gõ Nhanh](https://github.com/khaphanspace/gonhanh.org)** của **Kha Phan**.

Xin chân thành cảm ơn Kha Phan và các contributors của Gõ Nhanh đã tạo ra nền tảng tuyệt vời này. FKey kế thừa và tiếp nối sứ mệnh mang đến bộ gõ tiếng Việt chất lượng cao cho cộng đồng.

Dự án này cũng là sự tiếp nối từ **UniKey**, **OpenKey** và **EVKey**.

---

## 📄 License

FKey được phân phối theo giấy phép [BSD-3-Clause](LICENSE).

Bản quyền gốc © 2025 Gõ Nhanh Contributors.
