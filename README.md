<h1 align="center">
  <img src="assets/logo.png" alt="FKey Logo" width="128" height="128"><br>
  FKey - Bộ gõ Tiếng Việt
</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Windows-0078D6?logo=windows&logoColor=white" alt="Windows" />
  <img src="https://img.shields.io/github/v/release/miken90/fkey?color=green" alt="Release" />
  <img src="https://img.shields.io/github/downloads/miken90/fkey/total?color=blue" alt="Downloads" />
  <img src="https://img.shields.io/badge/License-BSD--3--Clause-blue.svg" alt="License" />
</p>

<p align="center">
  <strong>Bộ gõ tiếng Việt miễn phí, nhanh, nhẹ cho Windows</strong><br>
  ~11MB · Không cần cài đặt · Không quảng cáo · Không thu thập dữ liệu
</p>

<p align="center">
  <a href="https://github.com/miken90/fkey/releases/latest" target="_blank">
    <img src="https://img.shields.io/badge/📥_Tải_về-FKey_Portable-success?style=for-the-badge" alt="Download" />
  </a>
</p>

---

## 💎 Sponsor

<table>
  <tr>
    <td>
      <a href="https://vibecodecheap.com/?ref=SAUDF092" target="_blank">
        <img src="assets/vibecodecheap-banner.png" alt="VibeCodeCheap"/>
      </a>
    </td>
  </tr>
  <tr>
    <td>
      <strong><a href="https://vibecodecheap.com/?ref=SAUDF092" target="_blank">VibeCodeCheap</a> - AI Coding Subscriptions up to 80% OFF</strong><br/><br/>
      🎯 <strong>Full support for Amp Code & Claude Code</strong><br/>
      💰 <strong>$7.99/mo</strong> (vs $20 Claude Pro) | <strong>$29.99/mo</strong> (vs $100 Max 5x)<br/>
      ✅ Claude Opus 4.5, Sonnet 4.5, Gemini 3 Pro | 99.9% Uptime<br/><br/>
      <a href="https://vibecodecheap.com/?ref=SAUDF092" target="_blank"><strong>👉 Subscribe Now</strong></a>
    </td>
  </tr>
</table>

---

## ✨ Tính năng nổi bật

| Tính năng | Mô tả |
|-----------|-------|
| ⚡ **Siêu nhẹ** | ~11MB portable, ~18MB RAM |
| 🔍 **Mọi ứng dụng** | Chrome, VS Code, Terminal, Discord, Slack... |
| 🔤 **Auto-restore tiếng Anh** | `text` `expect` `user` → tự khôi phục khi nhấn Space |
| ⎋ **ESC khôi phục** | Gõ sai → nhấn ESC → về lại chữ gốc |
| 🔠 **Tự viết hoa** | Đầu câu tự động viết hoa |
| ⌨️ **Telex & VNI** | Chọn kiểu gõ quen thuộc |
| 🎯 **Đặt dấu chuẩn** | `hoà`, `khoẻ`, `thuỷ` |
| 🔧 **Phím tắt tùy chỉnh** | Ctrl+Space hoặc tuỳ ý |
| 🚀 **Auto-start** | Khởi động cùng Windows |
| 🔄 **Tự động cập nhật** | Thông báo khi có phiên bản mới |
| 📋 **Smart Paste** | Ctrl+Shift+V sửa lỗi mojibake tiếng Việt |
| 🛡️ **Chạy quyền Admin** | Gõ tiếng Việt trong ứng dụng yêu cầu quyền Admin |

---

## 📥 Tải về & Cài đặt

**Yêu cầu:** Windows 10/11 (64-bit), WebView2 Runtime

1. Tải <a href="https://github.com/miken90/fkey/releases/latest" target="_blank"><code>FKey-vX.X.X-portable.zip</code></a> từ GitHub Releases
2. Giải nén vào thư mục bất kỳ
3. Chạy `FKey.exe`
4. App chạy trong system tray (khay hệ thống)

---

## 🛡️ Cam kết

- 🚫 **Không thu phí** — Miễn phí mãi mãi
- 🚫 **Không quảng cáo** — Không popup
- 🚫 **Không theo dõi** — Offline 100%, mã nguồn mở

---

## 🔧 Tech Stack & Build

### Tech Stack

| Layer | Technology |
|-------|-----------|
| **Core Engine** | Rust |
| **App** | Go + Wails v3 + WebView2 |
| **Keyboard** | Win32 Low-level Hook |
| **Config** | Windows Registry |

### Build từ source

```powershell
cd core && cargo build --release
cd platforms\windows-wails && .\build.ps1 -Release
```

---

## ☕ Ủng hộ dự án

Nếu FKey hữu ích với bạn, hãy cân nhắc ủng hộ để duy trì và phát triển dự án:

<p align="center">
  <a href="https://me.momo.vn/2pIounFNtQTBIau6IlfR" target="_blank">
    <img src="https://img.shields.io/badge/MoMo-Ủng_hộ_qua_MoMo-ae2070?style=for-the-badge&logo=data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI+PHBhdGggZmlsbD0id2hpdGUiIGQ9Ik0xMiAyQzYuNDggMiAyIDYuNDggMiAxMnM0LjQ4IDEwIDEwIDEwIDEwLTQuNDggMTAtMTBTMTcuNTIgMiAxMiAyek0xMiAxOGMtMy4zMSAwLTYtMi42OS02LTZzMi42OS02IDYtNiA2IDIuNjkgNiA2LTIuNjkgNi02IDZ6Ii8+PC9zdmc+" alt="MoMo" />
  </a>
</p>

Cảm ơn bạn đã ủng hộ! 💖

---

## 🙏 Lời cảm ơn

FKey được phát triển dựa trên nền tảng của dự án <strong><a href="https://github.com/khaphanspace/gonhanh.org" target="_blank">Gõ Nhanh</a></strong> bởi <strong>Kha Phan</strong>.

Cảm ơn Kha Phan và cộng đồng Gõ Nhanh đã tạo ra engine xử lý tiếng Việt tuyệt vời. FKey tiếp nối sứ mệnh mang đến bộ gõ chất lượng cao, miễn phí cho người Việt.

---

## 📄 License

[BSD-3-Clause](LICENSE) © 2024-2026 FKey Contributors
