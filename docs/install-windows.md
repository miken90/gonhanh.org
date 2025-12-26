# Gõ Nhanh trên Windows

> **Production Ready** - Windows 10/11 (v1.0+)

Gõ Nhanh for Windows is a production-ready Vietnamese Input Method Engine built with WPF/.NET 8 and powered by the same high-performance Rust core engine used across all platforms.

---

## Yêu cầu hệ thống

- **Hệ điều hành**: Windows 10 (1809+) hoặc Windows 11
- **.NET Runtime**: Không cần cài đặt (self-contained)
- **RAM**: Tối thiểu 4GB
- **Dung lượng**: ~70MB (single-file portable)

---

## Tải xuống

### Tải thủ công (Khuyến nghị)

1. Tải file `.zip` mới nhất từ trang Releases:
   - [**Releases**](https://github.com/khaphanspace/gonhanh.org/releases/latest)
   - File: `GoNhanh-Windows-v{version}.zip`

2. Giải nén vào thư mục mong muốn (ví dụ: `C:\Program Files\GoNhanh`)

3. Chạy `GoNhanh.exe`

---

## Hướng dẫn cài đặt

### Lần đầu chạy

1. **Khởi động ứng dụng**: Double-click `GoNhanh.exe`

2. **Setup Wizard** (OnboardingWindow) sẽ xuất hiện với 3 trang:
   - **Trang 1**: Giới thiệu ứng dụng
   - **Trang 2**: Chọn phương thức gõ (Telex/VNI)
   - **Trang 3**: Cấu hình AutoStart (tùy chọn khởi động cùng Windows)

3. **Hoàn tất**: Click "Finish" để bắt đầu sử dụng

4. **System Tray Icon**: Biểu tượng Gõ Nhanh sẽ xuất hiện ở system tray (góc dưới phải màn hình)

### Kiểm tra hoạt động

Mở bất kỳ ứng dụng nào (Notepad, Word, Chrome) và thử gõ:

**Telex:**
- `as` → `á`
- `viet nam` → `việt nam`
- `khong` → `không`

**VNI:**
- `a1` → `á`
- `viet5 nam` → `việt nam`
- `kho7ng` → `không`

---

## Tính năng

### Cơ bản

- **Telex & VNI**: Hỗ trợ cả hai phương thức gõ phổ biến
- **Gõ tắt**: Mặc định: `vn` → `Việt Nam`, `ko` → `không`, `dc` → `được`
- **Tự động theo chế độ**: Gõ Nhanh tự động bật/tắt theo ứng dụng đang dùng
- **Validation-first**: Kiểm tra ngữ pháp tiếng Việt trước khi biến đổi
- **Hiệu suất cao**: Độ trễ \<1ms, RAM ~5MB

### Nâng cao (Advanced Settings)

Truy cập: Tray Icon → Advanced Settings

1. **Skip W Shortcut**: Tắt phím tắt `w` → `ư` trong Telex (mặc định: OFF)
2. **ESC Restore**: Nhấn ESC để khôi phục chữ gốc (mặc định: ON)
3. **Free Tone**: Cho phép đặt dấu tự do, bỏ qua validation (mặc định: OFF)
4. **English Auto-Restore**: Tự nhận diện và khôi phục từ tiếng Anh (`text`, `expect`, `window`) (mặc định: OFF)
5. **Auto-Capitalize**: Tự động viết hoa sau dấu câu `.` `!` `?` Enter (mặc định: ON)

### Phím tắt toàn cục (Global Hotkey)

**Mặc định**: `Ctrl+Space` - Chuyển đổi giữa tiếng Việt/tiếng Anh

**Cấu hình hotkey**:
1. Tray Icon → Advanced Settings
2. Phần "Global Hotkey"
3. Click vào ô hiển thị phím tắt hiện tại
4. Nhấn tổ hợp phím mới (ví dụ: `Ctrl+Shift+V`)
5. Click "Save"

**Hạn chế**: Không thể dùng phím tắt hệ thống (Ctrl+C/V/X/A/Z/Y, Alt+Tab/F4)

### Auto-Start

Cấu hình khởi động cùng Windows:
- **Trong setup wizard**: Tích checkbox ở trang 3
- **Sau khi cài**: Tray Icon → Advanced Settings → AutoStart toggle

---

## Cấu hình nâng cao

### Quản lý gõ tắt (Shortcuts)

Tray Icon → Settings → Shortcuts (nếu có UI) hoặc chỉnh sửa Registry:

**Registry path**: `HKCU\SOFTWARE\GoNhanh\Shortcuts`

Mỗi shortcut là một giá trị REG_SZ:
- **Name**: Từ gõ tắt (ví dụ: `vn`)
- **Data**: Kết quả (ví dụ: `Việt Nam`)

### Thay đổi phương thức gõ

Tray Icon → Input Method → Chọn Telex hoặc VNI

### Bật/Tắt tạm thời

- **Hotkey**: Nhấn `Ctrl+Space` (hoặc hotkey đã cấu hình)
- **Tray Icon**: Click chuột phải → Enable/Disable

---

## Gỡ bỏ cài đặt (Uninstall)

1. Tắt ứng dụng: Tray Icon → Exit
2. Xóa thư mục cài đặt (ví dụ: `C:\Program Files\GoNhanh`)
3. (Tùy chọn) Xóa cấu hình Registry: `HKCU\SOFTWARE\GoNhanh`

---

## Quy tắc gõ

### Telex

| Gõ | Kết quả | Giải thích |
|:---|:--------|:-----------|
| `as`, `af`, `ar`, `ax`, `aj` | `á`, `à`, `ả`, `ã`, `ạ` | Dấu thanh (sắc, huyền, hỏi, ngã, nặng) |
| `aa`, `ee`, `oo` | `â`, `ê`, `ô` | Dấu mũ (circumflex) |
| `aw`, `ow`, `uw` | `ă`, `ơ`, `ư` | Dấu móc/ngang (breve/horn) |
| `dd` | `đ` | Chữ đ |
| `w` (đầu từ) | `ư` | Phím tắt w→ư (có thể tắt) |

### VNI

| Gõ | Kết quả | Giải thích |
|:---|:--------|:-----------|
| `a1`, `a2`, `a3`, `a4`, `a5` | `á`, `à`, `ả`, `ã`, `ạ` | Dấu thanh (1-5) |
| `a6`, `e6`, `o6` | `â`, `ê`, `ô` | Dấu mũ (6) |
| `a8`, `o7`, `u7` | `ă`, `ơ`, `ư` | Dấu móc/ngang (7-8) |
| `d9` | `đ` | Chữ đ (9) |

---

## Khắc phục sự cố

### Ứng dụng không khởi động

1. **Kiểm tra antivirus**: Thêm `GoNhanh.exe` vào danh sách ngoại lệ
2. **Chạy với quyền Administrator**: Chuột phải → Run as Administrator
3. **Kiểm tra Windows version**: Yêu cầu Windows 10 (1809+) hoặc Windows 11

### Không gõ được tiếng Việt

1. **Kiểm tra trạng thái**: Tray Icon có hiển thị "Enabled" không?
2. **Nhấn hotkey**: Thử `Ctrl+Space` để bật lại
3. **Khởi động lại ứng dụng**: Exit → Chạy lại `GoNhanh.exe`

### Hotkey không hoạt động

1. **Kiểm tra xung đột**: Tắt các ứng dụng khác dùng cùng hotkey
2. **Đổi hotkey**: Advanced Settings → Global Hotkey → Đổi sang tổ hợp khác
3. **Restart ứng dụng**: Khởi động lại để áp dụng thay đổi

### Gõ bị dính/sai chữ

1. **Tắt IME khác**: Windows Settings → Time & Language → Language → Remove other IMEs
2. **Tắt Auto-correct**: Trong ứng dụng (Word, Chrome) tắt tính năng autocorrect
3. **Bật Free Tone**: Advanced Settings → Free Tone (nếu muốn đặt dấu tự do)

---

## Tính năng sắp ra mắt

- ✅ Gõ tắt tùy chỉnh (hoàn thành)
- ✅ Auto-start cùng Windows (hoàn thành)
- ✅ Global hotkey toggle (hoàn thành - v1.0+)
- 🔄 Danh sách app ngoại lệ (đang phát triển)
- 🔄 GUI quản lý shortcuts (đang phát triển)

---

## Liên hệ & Hỗ trợ

- **GitHub Issues**: [https://github.com/khaphanspace/gonhanh.org/issues](https://github.com/khaphanspace/gonhanh.org/issues)
- **Discussions**: [https://github.com/khaphanspace/gonhanh.org/discussions](https://github.com/khaphanspace/gonhanh.org/discussions)
- **Releases**: [https://github.com/khaphanspace/gonhanh.org/releases](https://github.com/khaphanspace/gonhanh.org/releases)

---

## Dành cho Developer

### Build từ source

**Yêu cầu**:
- Windows 10/11
- [Rust](https://rustup.rs/) (toolchain latest stable)
- [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0)
- Visual Studio 2022 (hoặc Build Tools for Visual Studio 2022)

**Build steps**:
```powershell
# Clone repository
git clone https://github.com/khaphanspace/gonhanh.org.git
cd gonhanh.org

# Build Rust core (DLL)
cd core
cargo build --release --target x86_64-pc-windows-msvc
copy target\x86_64-pc-windows-msvc\release\gonhanh_core.dll ..\platforms\windows\GoNhanh\libgonhanh_core.dll

# Build WPF application
cd ..\platforms\windows
dotnet build GoNhanh.sln --configuration Release

# Run application
cd GoNhanh\bin\Release\net8.0-windows
.\GoNhanh.exe
```

**Build script** (PowerShell):
```powershell
.\scripts\build-windows.ps1
```

### Kiến trúc

```
GoNhanh.exe (WPF/.NET 8)
    ├─ Core/RustBridge.cs → P/Invoke FFI
    ├─ Core/KeyboardHook.cs → SetWindowsHookEx (WH_KEYBOARD_LL)
    ├─ Core/KeyboardShortcut.cs → Hotkey model
    ├─ Controls/HotkeyRecorder.xaml → Hotkey recording UI
    ├─ Services/SettingsService.cs → Registry persistence
    └─ libgonhanh_core.dll (Rust engine)
```

**Chi tiết**: Xem [system-architecture.md](system-architecture.md)

---

**Last Updated**: 2025-12-26
**Version**: v1.0+ (Production Ready)
**Platform**: Windows 10/11 (.NET 8)
