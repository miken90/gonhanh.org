# 🔨 GoNhanh Windows - Build & Test Guide

Hướng dẫn build và test GoNhanh Windows sau khi implement advanced features.

---

## 📋 Prerequisites

### Required Tools
- ✅ **Windows 10/11** (64-bit)
- ✅ **.NET 8 SDK** - Already installed (version 8.0.416)
- ✅ **Visual Studio 2022** (hoặc VS Code với C# extension)
- ✅ **Rust DLL** - Already built (`platforms/windows/GoNhanh/Native/gonhanh_core.dll`)

### Verify Prerequisites

```powershell
# Check .NET SDK
dotnet --version
# Expected: 8.0.x

# Check if DLL exists
Test-Path "platforms/windows/GoNhanh/Native/gonhanh_core.dll"
# Expected: True
```

---

## 🏗️ Build Instructions

### Option 1: Build từ Command Line (Recommended)

```powershell
# Navigate to Windows platform directory
cd platforms\windows\GoNhanh

# Clean previous builds
dotnet clean --configuration Debug

# Build Debug version
dotnet build --configuration Debug

# Or build Release version
dotnet clean --configuration Release
dotnet build --configuration Release
```

**Alternative: Use simple build script**
```powershell
# From repository root
.\build-simple.ps1
```

**Expected Output**:
```
Build succeeded.
    0 Warning(s)
    0 Error(s)
```

**Output Location**:
- Debug: `platforms/windows/GoNhanh/bin/Debug/net8.0-windows/GoNhanh.exe`
- Release: `platforms/windows/GoNhanh/bin/Release/net8.0-windows/GoNhanh.exe`

### Option 2: Build với Visual Studio

1. Mở `platforms/windows/GoNhanh.sln` trong Visual Studio 2022
2. Select configuration: **Debug** hoặc **Release**
3. Menu: **Build** → **Build Solution** (hoặc `Ctrl+Shift+B`)
4. Check Output window để verify build succeeded

### Option 3: Build với VS Code

1. Mở folder `platforms/windows/GoNhanh` trong VS Code
2. Press `Ctrl+Shift+B` để run build task
3. Select **.NET: build**
4. Check Terminal output

---

## ▶️ Run Application

### Run từ Command Line

```powershell
# Run Debug build
cd platforms/windows/GoNhanh/bin/Debug/net8.0-windows
.\GoNhanh.exe

# Run Release build
cd platforms/windows/GoNhanh/bin/Release/net8.0-windows
.\GoNhanh.exe
```

### Run từ Visual Studio

1. Set **GoNhanh** as startup project
2. Press `F5` (Debug mode) hoặc `Ctrl+F5` (Run without debugging)

### Expected Behavior on First Run

1. **System Tray Icon** xuất hiện (chữ "VN" hoặc "EN")
2. **Onboarding Window** hiện lên (nếu first run)
3. Click tray icon → menu hiện ra với options:
   - Input Method (Telex/VNI)
   - Enable/Disable
   - **Cài đặt nâng cao...** ← NEW!
   - Giới thiệu GoNhanh
   - Thoát

---

## 🧪 Testing Guide

### 1. Quick Smoke Test (2 phút)

**Test basic Vietnamese typing**:

1. Run GoNhanh
2. Mở **Notepad** (hoặc bất kỳ text editor nào)
3. Ensure IME is enabled (tray icon shows "VN")
4. Test Telex:
   ```
   Type: as    → Expect: á
   Type: aa    → Expect: â
   Type: aw    → Expect: ă
   Type: oo    → Expect: ô
   Type: ow    → Expect: ơ
   Type: uw    → Expect: ư
   Type: dd    → Expect: đ
   ```

5. Test VNI (switch method từ tray menu):
   ```
   Type: a1    → Expect: á
   Type: a2    → Expect: à
   Type: a3    → Expect: ả
   Type: a4    → Expect: ã
   Type: a5    → Expect: ạ
   ```

**✅ PASS nếu**: Tất cả ký tự Vietnamese hiển thị đúng

### 2. Advanced Features Test (10 phút)

**Test Advanced Settings UI**:

1. Right-click tray icon
2. Click **"Cài đặt nâng cao..."**
3. Window mở ra với:
   - 5 checkboxes (Skip W Shortcut, ESC Restore, Free Tone, English Auto-Restore, Auto-Capitalize)
   - Shortcuts DataGrid
   - Add/Remove buttons
   - Save/Cancel buttons

**Test Shortcuts**:

1. Trong Advanced Settings window:
   - Trigger: `vn`
   - Replacement: `Việt Nam`
   - Click **Thêm**
2. Click **Lưu**
3. Quay lại Notepad:
   ```
   Type: vn<space>  → Expect: Việt Nam
   ```

**Test Settings Persistence**:

1. Enable một vài settings (e.g., ESC Restore, Auto-Capitalize)
2. Click **Lưu**
3. **Restart GoNhanh** (Exit → Run lại)
4. Mở Advanced Settings lại
5. **✅ PASS nếu**: Settings vẫn được checked

### 3. ESC Restore Test

**Test ESC key restores original**:

1. Enable "ESC Restore" trong Advanced Settings
2. Trong Notepad:
   ```
   Type: user    → See: úẻ
   Press: ESC    → Expect: user (restored)
   ```

**✅ PASS nếu**: ESC key restores về raw ASCII

### 4. English Auto-Restore Test

1. Enable "English Auto-Restore" trong Advanced Settings
2. Trong Notepad:
   ```
   Type: text    → Expect: text (không thành tẽt)
   Type: expect  → Expect: expect (không thành ẽpẹct)
   Type: user    → Expect: user (không thành úẻ)
   ```

**✅ PASS nếu**: Common English words không bị transform

### 5. App Compatibility Test (15 phút)

Test typing trong các apps khác nhau:

| App | Test | Status |
|-----|------|--------|
| **Notepad** | Type `vietnam` | ⬜ |
| **MS Word** | Type `tiếng việt` | ⬜ |
| **Chrome** (Google Docs) | Type `xin chào` | ⬜ |
| **VS Code** | Type `// comment việt` | ⬜ |
| **Slack/Discord** | Type chat message | ⬜ |

**✅ PASS nếu**: Typing hoạt động trong tất cả apps

### 6. Registry Persistence Test

**Verify settings persist to Registry**:

1. Open **Registry Editor** (`regedit`)
2. Navigate to: `HKEY_CURRENT_USER\Software\GoNhanh`
3. Check values:
   - `InputMethod` (0 = Telex, 1 = VNI)
   - `Enabled` (1 = enabled)
   - `SkipWShortcut`, `EscRestore`, etc.
4. Check `HKEY_CURRENT_USER\Software\GoNhanh\Shortcuts` cho shortcuts

**✅ PASS nếu**: All settings có trong Registry

---

## 🐛 Troubleshooting

### Build Errors

**Error: "gonhanh_core.dll not found"**

Solution:
```powershell
# Copy DLL to output directory
Copy-Item "platforms/windows/GoNhanh/Native/gonhanh_core.dll" `
          "platforms/windows/GoNhanh/bin/Debug/net8.0-windows/win-x64/"
```

**Error: "CS0234: The type or namespace name does not exist"**

Solution: Clean và rebuild
```powershell
dotnet clean
dotnet build
```

### Runtime Errors

**App crashes on startup**

1. Check Event Viewer: `Windows Logs → Application`
2. Look for .NET Runtime errors
3. Verify DLL is in correct location

**Typing doesn't work**

1. Check tray icon shows "VN" (enabled)
2. Try switching method (Telex ↔ VNI)
3. Check keyboard hook is active (no admin apps blocking)

**Advanced Settings window doesn't open**

1. Check Debug output trong VS Output window
2. Verify SettingsService.cs và ShortcutsManager.cs built correctly

---

## 📊 Full Test Checklist

Copy checklist này để track testing progress:

```markdown
### Basic Functionality
- [ ] App starts without errors
- [ ] Tray icon appears
- [ ] Tray menu opens
- [ ] Can switch input methods (Telex/VNI)
- [ ] Can enable/disable IME

### Vietnamese Typing (Telex)
- [ ] á (as)
- [ ] à (af)
- [ ] ả (ar)
- [ ] ã (ax)
- [ ] ạ (aj)
- [ ] â (aa)
- [ ] ă (aw)
- [ ] ô (oo)
- [ ] ơ (ow)
- [ ] ư (uw)
- [ ] đ (dd)

### Vietnamese Typing (VNI)
- [ ] á (a1)
- [ ] à (a2)
- [ ] ả (a3)
- [ ] ã (a4)
- [ ] ạ (a5)
- [ ] â (a6)
- [ ] ô (o6)

### Advanced Features
- [ ] Advanced Settings window opens
- [ ] All 5 checkboxes present
- [ ] Shortcuts DataGrid displays
- [ ] Can add shortcut
- [ ] Can remove shortcut
- [ ] Save button persists settings
- [ ] Cancel button discards changes

### Feature Tests
- [ ] ESC Restore works (user → úẻ → ESC → user)
- [ ] English Auto-Restore (text stays text)
- [ ] Shortcuts expand (vn → Việt Nam)
- [ ] Auto-Capitalize after period

### Persistence
- [ ] Settings survive app restart
- [ ] Shortcuts survive app restart
- [ ] Registry values correct

### App Compatibility
- [ ] Notepad
- [ ] MS Word
- [ ] Chrome/Edge
- [ ] VS Code
- [ ] Other apps

### Performance
- [ ] Typing latency < 50ms
- [ ] No lag when typing fast
- [ ] CPU usage reasonable
```

---

## 📝 Report Issues

Nếu phát hiện bugs, tạo report với format:

```markdown
## Bug Report

**Environment**:
- Windows version:
- GoNhanh version:
- .NET version:

**Steps to Reproduce**:
1.
2.
3.

**Expected Behavior**:

**Actual Behavior**:

**Screenshots/Logs**:

**Registry State** (if relevant):
```

---

## ✅ Next Steps After Testing

Nếu tests PASS:

1. **Create commit**:
   ```bash
   git add .
   git commit -m "feat(windows): implement advanced Vietnamese typing features"
   ```

2. **Create GitHub release** (optional)

3. **Deploy** to production

Nếu có issues:
1. Document trong issue report
2. Fix bugs
3. Re-test
4. Repeat until all tests pass

---

## 📚 Additional Resources

- **Implementation Plan**: `plans/251225-1407-fix-windows-vietnamese-typing/plan.md`
- **Phase 2 Report**: `plans/reports/fullstack-developer-251225-1520-phase2-ffi-bindings.md`
- **Test Report**: `plans/reports/tester-251225-1528-phase5-testing.md`
- **Code Review**: `plans/reports/code-reviewer-251225-1542-final-review.md`
- **Docs Update**: `plans/reports/docs-manager-251225-1548-windows-advanced-features.md`

---

**Happy Testing! 🚀**
