# FKey: Project Overview & Product Development Requirements

## Project Vision

FKey (formerly Gõ Nhanh) is a **high-performance Vietnamese input method engine** (IME) for Windows, built on a zero-dependency Rust core. It enables fast, accurate Vietnamese text input with minimal system overhead.

The project demonstrates production-grade system software design:
- **Zero-dependency Rust core** (~3,500 LOC) - platform-agnostic, FFI-ready
- **WPF/.NET 8 UI** - Native Windows integration with system tray
- **Validation-first pipeline** - Vietnamese phonology rules checked BEFORE transformation
- **Smart auto-restore** - Detects English patterns (text, expect, user) and auto-reverts on space
- **App-aware injection** - Fast mode for standard apps, Slow mode for Electron/terminals

## Product Goals

1. **Performance**: Sub-millisecond keystroke latency (<1ms) - achieved at ~0.2-0.5ms
2. **Reliability**: Validation-first architecture (phonology rules checked BEFORE transformation)
3. **User Experience**: Seamless Windows integration (SetWindowsHookEx, SendInput)
4. **Memory Efficiency**: ~10MB memory footprint with optimized binary packaging

## Target Users

- **Primary**: Vietnamese developers, technical writers, students typing Vietnamese daily on Windows
- **Secondary**: Vietnamese diaspora, bilingual professionals working with code and Vietnamese text
- **Pain Points Solved**:
  - Need to toggle IME when typing English code/commands
  - Compatibility with Electron apps (VS Code, Notion, Discord)
  - Terminal support (Windows Terminal, PowerShell, cmd)
- **Requirements**: Windows 10+, no runtime dependencies (self-contained)

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
- **Windows**: SetWindowsHookEx (WH_KEYBOARD_LL) + SendInput for text injection
- **App-aware injection**: Fast mode for standard apps, Slow mode for Electron/terminals/browsers
- **Global hotkey**: Ctrl+Space for Vietnamese/English toggle (configurable)
- **Auto-start**: Windows auto-start on login via Registry

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
- **Windows**: Windows 10/11, .NET 8 runtime (self-contained)
- Works with all major applications: Notepad, Word, VS Code, Chrome, Edge, Office, JetBrains IDEs
- App-aware injection handles Electron apps and terminals

### Security
- No internet access required (offline-first, zero telemetry)
- BSD-3-Clause license (free and open source)
- Accessibility permission: Required for keyboard hook (transparent user prompt)
- No data collection, no analytics, no cloud sync
- All processing happens locally on device

## Architecture Overview

```
User Keystroke (SetWindowsHookEx WH_KEYBOARD_LL)
        ↓
   RustBridge.cs (P/Invoke FFI)
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
   TextSender.cs (SendInput + KEYEVENTF_UNICODE)
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

### Current: Windows Production (v1.6.0)
- ✅ SetWindowsHookEx keyboard hook
- ✅ WPF/.NET 8 UI with system tray
- ✅ Registry-based settings persistence
- ✅ 5 Advanced Settings (SkipWShortcut, EscRestore, FreeTone, EnglishAutoRestore, AutoCapitalize)
- ✅ Shortcuts Manager with Registry persistence
- ✅ Global Hotkey Toggle (configurable, default Ctrl+Space)
- ✅ Auto-Start on login via Registry
- ✅ Unicode text injection (clipboard-safe)
- ✅ Async keyboard processing (race condition fixed)

### Resolved Issues
- ✅ **Race condition with fast typing** (FIXED in Phase 3 - async queue architecture)
  - Previous symptoms: "hiện" → "hinệ", "không" → "kohng"
  - Root cause: Thread.Sleep() in TextSender blocked hook callback
  - Solution: Async queue architecture (Hook → Queue → Worker thread)
    - Hook enqueues events in <1μs (non-blocking)
    - Worker thread processes on background (Thread.Sleep doesn't block hook)
    - Implementation: `Core/KeyEventQueue.cs`, `Core/KeyboardWorker.cs`, `Core/KeyboardHook.cs`

### Future Enhancements
- User-editable shortcuts via UI (Registry backend ready)
- Dictionary lookup integration
- Custom diacritics placement rules

## Development Standards

### Code Organization
- **Core** (`core/src/`): Rust engine, pure logic, zero platform dependencies
- **Platform** (`platforms/windows/`): WPF UI, platform integration, FFI bridge
- **Scripts** (`scripts/`): Build automation
- **Tests** (`core/tests/`): Integration tests + unit tests

### Quality Gates
- Format: `cargo fmt` (automatic formatting)
- Lint: `cargo clippy -- -D warnings` (no warnings allowed)
- Tests: `cargo test` (700+ tests)
- Build: `dotnet publish -c Release`

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

### C#/Windows
- .NET 8: WPF for UI, P/Invoke for FFI
- Windows API: SetWindowsHookEx, SendInput, Registry

### Build Tools
- `cargo` (Rust toolchain)
- `dotnet` (Windows app build)

## Maintenance & Support

### Community
- GitHub Issues: Bug reports and feature requests
- GitHub Discussions: Questions and community support
- Contributing: BSD-3-Clause, open to contributions

---

**Last Updated**: 2025-12-30
**Status**: Active Development
**Current Version**: v1.6.0
**Platform**: Windows 10/11 (.NET 8, WPF)
**Repository**: https://github.com/khaphanspace/gonhanh.org

**Resolved Issues**:
- ✅ Race condition with fast typing (FIXED via async queue architecture)

**Complete Features**:
- ✅ Telex + VNI input methods
- ✅ 5 advanced settings
- ✅ Shortcuts Manager with Registry persistence
- ✅ Global hotkey toggle (configurable)
- ✅ Auto-start configuration
- ✅ Unicode text injection (clipboard-safe)
- ✅ Async queue keyboard processing (Phase 3 complete)
