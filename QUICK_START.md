# ⚡ Quick Start - GoNhanh Windows

Build và test nhanh GoNhanh Windows trong 5 phút.

## 🚀 Quick Build

**Option 1: Manual Commands (Recommended)**
```powershell
# Build
cd platforms\windows\GoNhanh
dotnet clean --configuration Debug
dotnet build --configuration Debug

# Run
cd bin\Debug\net8.0-windows
.\GoNhanh.exe
```

**Option 2: Simple Script**
```powershell
# From repository root
.\build-simple.ps1

# Then run
cd platforms\windows\GoNhanh\bin\Debug\net8.0-windows
.\GoNhanh.exe
```

## ✅ Quick Test (30 giây)

1. **Mở Notepad**
2. **Type**: `as` → Expect: `á` ✅
3. **Type**: `vietnam` → Expect: `việtnạm` ✅
4. **Right-click tray icon** → Click **"Cài đặt nâng cao..."**
5. **Add shortcut**: `vn` → `Việt Nam`
6. **Save**, quay Notepad
7. **Type**: `vn ` → Expect: `Việt Nam ` ✅

## 🎯 Done!

Nếu 7 steps trên PASS → Implementation hoạt động đúng! 🎉

Full guide: `BUILD_AND_TEST_GUIDE.md`
