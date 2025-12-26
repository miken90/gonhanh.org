# Gõ Nhanh: Project Overview & Product Development Requirements

## Project Vision

Gõ Nhanh is a **high-performance Vietnamese input method engine** (IME) with multi-platform support (macOS, Windows, Linux) built on a zero-dependency Rust core. It enables fast, accurate Vietnamese text input with minimal system overhead while addressing critical bugs in existing IMEs (Chrome/Spotlight address bar issues, autocomplete conflicts).

The project demonstrates production-grade system software design:
- **Zero-dependency Rust core** (~3,500 LOC) - platform-agnostic, FFI-ready
- **Native UI per platform** - SwiftUI (macOS), WPF/.NET 8 (Windows), Fcitx5 (Linux)
- **Validation-first pipeline** - Vietnamese phonology rules checked BEFORE transformation
- **Smart auto-restore** - Detects English patterns (text, expect, user) and auto-reverts on space
- **Per-app mode memory** - Remembers IME on/off state per application

## Product Goals

1. **Performance**: Sub-millisecond keystroke latency (<1ms) - achieved at ~0.2-0.5ms
2. **Reliability**: Validation-first architecture (phonology rules checked BEFORE transformation)
3. **Cross-Platform**: macOS + Windows (production) + Linux (beta) with consistent core engine
4. **User Experience**: Seamless platform integration (CGEventTap on macOS, SetWindowsHookEx on Windows)
5. **Memory Efficiency**: ~5MB memory footprint with optimized binary packaging

## Target Users

- **Primary**: Vietnamese developers, technical writers, students typing Vietnamese daily on macOS/Windows/Linux
- **Secondary**: Vietnamese diaspora, bilingual professionals working with code and Vietnamese text
- **Pain Points Solved**:
  - Chrome/Arc/Spotlight address bar diacritics issues (fixed with Selection method)
  - Autocomplete conflicts in IDEs (JetBrains, Excel)
  - Need to toggle IME when typing English code/commands
  - Per-app IME state (e.g., OFF in VS Code, ON in Slack)
- **Requirements**: macOS 10.15+ / Windows 10+ / Linux with Fcitx5, Accessibility permissions

## Core Functional Requirements

### Input Methods
- **Telex**: Vietnamese VIQR-style (s/f/r/x/j for tones, w for horn, dd→đ)
- **VNI**: Alternative numeric layout (1-5 for tones, 6-8 for marks, 9 for đ)
- **Shortcuts**: User-defined abbreviations (vn→Việt Nam, ko→không)
- **Auto-restore English**: Smart detection of English patterns (text, expect, user, window) with auto-revert on space
- **ESC to revert**: Press ESC to restore original text without disabling IME

### Keystroke Processing
1. Buffer management: Maintain context for multi-keystroke transforms
2. Validation: Check syllable against Vietnamese phonology rules
3. Transformation: Apply diacritics (sắc, huyền, hỏi, ngã, nặng) and tone modifiers (circumflex, horn)
4. Output: Send backspace + replacement characters or pass through

### Platform Integration
- **macOS**: CGEventTap keyboard hook with dual text replacement (Backspace method for body text, Selection method for address bars/autocomplete)
- **Windows**: SetWindowsHookEx (WH_KEYBOARD_LL) + SendInput for text injection
- **Linux**: Fcitx5 addon integration with X11/Wayland support
- **Smart context detection**: Accessibility API detects focused element (AXComboBox, AXSearchField) to choose replacement method
- **Global hotkey**: Ctrl+Space for Vietnamese/English toggle (configurable)
- **Per-app IME state**: Remembers enabled/disabled state per application bundle ID
- **Auto-follow input source**: Auto-disables when switching to Japanese/Korean/Chinese input

## Non-Functional Requirements

### Performance
- Keystroke latency: <1ms measured end-to-end
- CPU usage: <2% during normal typing
- Memory: ~5MB resident set size
- No input delay under sustained high-speed typing

### Reliability
- 600+ integration tests covering edge cases (Telex, VNI, auto-restore, shortcuts)
- Validation-first pattern: Reject invalid Vietnamese before transforming
- Graceful fallback: Pass through on disable or invalid input
- Thread-safe global engine instance via Mutex
- No panics in FFI boundary (all errors handled)

### Compatibility
- **macOS**: 10.15 Catalina and later, Apple Silicon (arm64) + Intel (x86_64) universal binaries
- **Windows**: Windows 10/11, .NET 8 runtime
- **Linux**: Fcitx5 framework, X11/Wayland
- Works with all major applications: Terminal, VS Code, Chrome, Safari, Office, JetBrains IDEs
- Smart handling of autocomplete contexts (address bars, search fields, IDE completions)

### Security
- No internet access required (offline-first, zero telemetry)
- BSD-3-Clause license (free and open source)
- Accessibility permission: Required for keyboard hook (transparent user prompt)
- No data collection, no analytics, no cloud sync
- All processing happens locally on device

## Architecture Overview

```
User Keystroke (CGEventTap/SetWindowsHookEx)
        ↓
   Platform Bridge (RustBridge.swift / RustBridge.cs)
        ↓
   Rust Engine (ime_key) - Validation-First 7-Stage Pipeline
    ├─ Stage 1: Stroke Detection (đ/Đ)
    ├─ Stage 2: Tone Mark Detection (sắc/huyền/hỏi/ngã/nặng)
    ├─ Stage 3: Vowel Mark Detection (circumflex/horn/breve)
    ├─ Stage 4: Mark Removal (revert previous marks)
    ├─ Stage 5: W-Vowel Handling (Telex-specific, "w"→"ư")
    ├─ Stage 6: Normal Letter Processing (pass-through)
    └─ Stage 7: Shortcut Expansion (user-defined abbreviations)
        ↓
   Validation (5 Vietnamese Phonology Rules) - Applied BEFORE Transform
    ├─ Rule 1: Must have vowel
    ├─ Rule 2: Valid initial consonants only
    ├─ Rule 3: All characters parsed
    ├─ Rule 4: Spelling rules (c/k/g restrictions)
    └─ Rule 5: Valid final consonants only
        ↓
   Transform & Output (apply diacritics, tone marks)
        ↓
   Result (action, backspace count, output chars)
        ↓
   Platform UI (SwiftUI on macOS, WPF on Windows - Send text or pass through)
```

## Success Metrics

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Keystroke latency | <1ms | ~0.2-0.5ms | ✓ Exceeds |
| Memory usage | <10MB | ~5MB | ✓ Exceeds |
| Test coverage | >90% | 600+ tests, 19 test files | ✓ Exceeds |
| Platform coverage | 3 platforms | macOS (prod), Windows (prod), Linux (beta) | ✓ Met |
| Code quality | Zero warnings | `cargo clippy -D warnings` | ✓ Met |
| Cross-platform core | Single codebase | Rust core ~3,500 LOC, FFI-ready | ✓ Met |

## Roadmap

### Phase 1: macOS (Complete - v1.0.89+)
- Telex + VNI input methods
- Menu bar app with settings
- Auto-launch on login (SMAppService)
- Update checker via GitHub releases
- Validation-first architecture
- Shortcut system with priority matching
- Auto-restore English patterns
- Per-app IME state memory
- Smart text replacement (Backspace + Selection methods)
- ESC to revert transformation

### Phase 2: Cross-Platform (Windows Complete, Linux In Progress)

**Windows 10/11 (Complete - Production Ready, Feature Parity + Global Hotkey)**
- SetWindowsHookEx keyboard hook
- WPF/.NET 8 UI with system tray
- Registry-based settings persistence
- **5 Advanced Settings**:
  - Skip W Shortcut (w→ư control in Telex)
  - ESC Restore (ESC reverts to raw ASCII)
  - Free Tone (skip validation, place diacritics anywhere)
  - English Auto-Restore (auto-detect English words)
  - Auto-Capitalize (capitalize after . ! ? Enter)
- **Shortcuts Manager**:
  - User-defined abbreviations with Registry persistence
  - Default Vietnamese abbreviations (vn, hn, hcm, ko, dc, vs, ms)
  - Auto-sync with Rust engine via 11 FFI methods
- **Advanced Settings UI**:
  - AdvancedSettingsWindow.xaml for configuration
  - Full feature compatibility with macOS version
- **Global Hotkey Toggle** (NEW - 2025-12-26):
  - Configurable keyboard shortcut (default: Ctrl+Space)
  - KeyboardShortcut model with Registry serialization
  - HotkeyRecorder UserControl with keycap-style UI
  - System shortcut conflict detection
  - OnHotkeyTriggered event integration in KeyboardHook
  - App.xaml.cs wiring for hotkey → toggle IME state
- **Auto-Start Configuration** (NEW - 2025-12-26):
  - Windows auto-start on login via Registry
  - UI in OnboardingWindow (Page 3) and AdvancedSettingsWindow
  - SettingsService management
- Compiled DLL shared with macOS core

**Linux (Beta)**
- Fcitx5 addon integration
- C++ bridge to Rust core
- X11/Wayland support
- Feature parity with macOS/Windows

### Phase 3: Enhanced Features (Future)
- Advanced auto-restore heuristics (ML-based pattern detection)
- User-editable shortcuts via UI
- Dictionary lookup integration
- Custom diacritics placement rules
- Cloud sync for preferences (optional, privacy-first)
- Mobile support (iOS/Android with platform-specific UI)

## Development Standards

### Code Organization
- **Core** (`core/src/`): Rust engine, pure logic, zero platform dependencies
- **Platform** (`platforms/macos/`): SwiftUI UI, platform integration, FFI bridge
- **Scripts** (`scripts/`): Build automation for universal binaries
- **Tests** (`core/tests/`): Integration tests + unit tests

### Quality Gates
- Format: `cargo fmt` (automatic formatting, enforced by CI)
- Lint: `cargo clippy -- -D warnings` (no warnings allowed)
- Tests: `cargo test` (600+ tests must pass across 19 test files)
- Build: Universal binary creation (arm64 + x86_64 for macOS)
- CI/CD: GitHub Actions runs format, clippy, test, build on every push/PR

### Commit Message Format
Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): subject

body

footer
```

Examples:
- `feat(engine): add shortcut expansion for common abbreviations`
- `fix(transform): correct diacritic placement for ư vowel`
- `docs(ffi): update RustBridge interface documentation`
- `test(validation): add edge cases for invalid syllables`

## Dependencies

### Rust
- Zero production dependencies (pure stdlib for core engine)
- Dev: `rstest` for parametrized tests, `serial_test` for sequential test execution

### Swift/macOS
- Foundation: URLSession, UserDefaults, FileHandle
- AppKit: NSApplication, NSStatusBar, CGEventTap, Accessibility API
- SwiftUI: Standard UI components, macOS 11+ features

### C#/Windows
- .NET 8: WPF for UI, P/Invoke for FFI
- Windows API: SetWindowsHookEx, SendInput, Registry

### C++/Linux
- Fcitx5: Input method framework
- X11/Wayland: Display server protocols

### Build Tools
- `cargo` (Rust toolchain)
- `xcodebuild` (macOS app build)
- `dotnet` (Windows app build)
- `cmake` (Linux Fcitx5 addon)
- GNU Make (build automation)
- GitHub Actions (CI/CD)

## Maintenance & Support

### Release Schedule
- Patch releases: Bug fixes and small improvements (monthly)
- Minor releases: New features (quarterly)
- Major releases: Breaking changes (annually or as needed)

### Community
- GitHub Issues: Bug reports and feature requests
- GitHub Discussions: Questions and community support
- Contributing: BSD-3-Clause, open to contributions
- Sponsorship: GitHub Sponsors for financial support

## Success Criteria for Milestones

**v1.0 Release** (Achieved)
- All core input methods working reliably
- Sub-1ms latency confirmed (~0.2-0.5ms)
- 600+ tests passing across 19 test files
- macOS app in official release (Homebrew available)
- Production-ready on macOS and Windows

**v1.1+ Releases** (Current)
- Cross-platform support (macOS ✓, Windows ✓ feature parity + hotkey, Linux in progress)
- User-customizable shortcuts (implemented on both platforms)
- **Windows Advanced Features**:
  - 5 advanced settings (SkipWShortcut, EscRestore, FreeTone, EnglishAutoRestore, AutoCapitalize)
  - Shortcuts Manager with Registry persistence
  - AdvancedSettingsWindow UI
  - 11 RustBridge FFI methods
  - **Global hotkey toggle (NEW - 2025-12-26)**
  - **Auto-start configuration (NEW - 2025-12-26)**
- Enhanced documentation (ongoing)
- Community contribution guidelines (CONTRIBUTING.md)
- Auto-restore English patterns (implemented)
- Per-app IME state memory (implemented)

---

**Last Updated**: 2025-12-26
**Status**: Active Development
**Current Version**: v1.0.89
**Platforms**: macOS (production), Windows (production, feature parity + hotkey toggle), Linux (beta)
**Repository**: https://github.com/khaphanspace/gonhanh.org

**Windows Platform Recent Updates (2025-12-26)**:
- ✅ **Unicode text injection fix** - Replaced clipboard-based injection with SendInput API + KEYEVENTF_UNICODE
  - Preserves user's clipboard content when typing Vietnamese
  - Fixes clipboard override issue reported by users
- ✅ **Uppercase detection fix** - Engine checks both CapsLock AND Shift (mod.rs:718)
  - Shift+DD now correctly produces uppercase Đ (was lowercase đ)
  - Fixes uppercase loss with diacritical characters
- ✅ Global hotkey toggle (Ctrl+Space default, configurable)
- ✅ KeyboardShortcut model with Registry serialization
- ✅ HotkeyRecorder UserControl with keycap-style UI and conflict detection
- ✅ AutoStart UI in OnboardingWindow (Page 3) and AdvancedSettingsWindow
- ✅ KeyboardHook OnHotkeyTriggered event integration
- ✅ App.xaml.cs wiring for hotkey → toggle IME

**Windows Platform Complete Features**:
- ✅ Feature parity with macOS achieved
- ✅ 5 advanced settings implemented
- ✅ Shortcuts Manager with Registry persistence
- ✅ Advanced Settings UI
- ✅ Global hotkey toggle
- ✅ Auto-start configuration
- ✅ Unicode text injection (clipboard-safe, uppercase-correct)
- ✅ Production-ready build
