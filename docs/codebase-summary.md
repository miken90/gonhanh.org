# Gõ Nhanh: Codebase Summary

Complete directory structure, module responsibilities, and development entry points for the Gõ Nhanh Vietnamese Input Method Engine.

## Directory Structure

```
gonhanh.org/
├── core/                          # Rust engine (100% platform-agnostic)
│   ├── src/
│   │   ├── lib.rs                # FFI exports (ime_init, ime_key, ime_method, etc.)
│   │   ├── utils.rs              # Utility functions (char conversions, etc.)
│   │   │
│   │   ├── engine/               # Core processing pipeline
│   │   │   ├── mod.rs            # Main Engine struct + ime_key orchestration
│   │   │   ├── buffer.rs         # Circular typing buffer (64 chars)
│   │   │   ├── syllable.rs       # Syllable parsing (C+G+V+C pattern)
│   │   │   ├── validation.rs     # Vietnamese phonology rules (5 rules)
│   │   │   ├── transform.rs      # Diacritic + tone application (pattern-based)
│   │   │   └── shortcut.rs       # User-defined abbreviations with priority
│   │   │
│   │   ├── input/                # Input method strategies
│   │   │   ├── mod.rs            # Input trait + method registry
│   │   │   ├── telex.rs          # Telex method (a/e/o/w for tones, s/f/r/x/j for marks)
│   │   │   └── vni.rs            # VNI method (1-5 for marks, 6-8 for tones, 9 for đ)
│   │   │
│   │   └── data/                 # Static Vietnamese linguistic data
│   │       ├── mod.rs            # Data module exports
│   │       ├── keys.rs           # Telex/VNI keycode to transformation mappings
│   │       ├── chars.rs          # Character data (UTF-32 constants, casing)
│   │       ├── vowel.rs          # Vowel table (72 entries: 12 bases × 6 marks)
│   │       └── constants.rs      # Constants (consonants, valid clusters, etc.)
│   │
│   ├── tests/                    # Integration + unit tests (2100+ lines)
│   │   ├── common/mod.rs         # Test utilities (IME helper, test setup)
│   │   ├── unit_test.rs          # Unit tests for individual modules
│   │   ├── typing_test.rs        # Full keystroke sequences (Telex + VNI)
│   │   ├── engine_test.rs        # Engine initialization + state tests
│   │   ├── integration_test.rs   # End-to-end keystroke→output tests
│   │   └── paragraph_test.rs     # Multi-word paragraph typing tests
│   │
│   └── Cargo.toml               # Rust dependencies (zero production deps)
│
├── platforms/                    # Platform-specific implementations
│   │
│   ├── macos/                   # Production: SwiftUI app (~1700 LOC)
│   │   ├── App.swift            # AppDelegate + main application setup
│   │   ├── RustBridge.swift     # FFI bridge to Rust engine (CRITICAL)
│   │   ├── MenuBar.swift        # Status bar UI + menu items
│   │   │
│   │   ├── SettingsView.swift   # Input method selection + preferences
│   │   ├── OnboardingView.swift # Accessibility permission setup wizard
│   │   ├── AboutView.swift      # About window + version info
│   │   ├── UpdateView.swift     # Update notification UI
│   │   │
│   │   ├── LaunchAtLogin.swift  # SMAppService integration (auto-launch)
│   │   ├── UpdateManager.swift  # DMG download + version tracking
│   │   ├── UpdateChecker.swift  # GitHub API integration (version checking)
│   │   ├── AppMetadata.swift    # Shared app constants (version, names)
│   │   │
│   │   ├── libgonhanh_core.a    # Compiled universal Rust library (arm64 + x86_64)
│   │   ├── GoNhanh.xcodeproj/   # Xcode project + build settings
│   │   ├── Assets.xcassets/     # App icons (1024×1024 down to 16×16)
│   │   ├── dmg-resources/       # DMG installer background + resources
│   │   └── Tests/               # Swift unit tests (LaunchAtLoginTests.swift)
│   │
│   ├── windows/                 # Production: WPF/.NET 8 app (~1500 LOC)
│   │   ├── App.xaml.cs          # Application entry point + global hotkey wiring
│   │   ├── Core/
│   │   │   ├── RustBridge.cs    # FFI bridge to Rust engine (11 FFI methods)
│   │   │   ├── KeyboardHook.cs  # SetWindowsHookEx + global hotkey detection
│   │   │   ├── KeyboardShortcut.cs  # Hotkey model with KeyCode+Modifiers
│   │   │   ├── KeyCodes.cs      # Windows virtual keycodes mapping
│   │   │   ├── TextSender.cs    # Text input simulation (SendInput)
│   │   │   └── AppMetadata.cs   # Shared app constants (version, names)
│   │   ├── Services/
│   │   │   ├── SettingsService.cs   # Registry settings + ToggleHotkey property
│   │   │   ├── UpdateService.cs     # Windows update checker
│   │   │   └── ShortcutsManager.cs  # User shortcuts + Registry persistence
│   │   ├── Views/
│   │   │   ├── TrayIcon.cs      # System tray icon UI + menu
│   │   │   ├── OnboardingWindow.xaml.cs # Setup wizard + AutoStart checkbox
│   │   │   ├── AboutWindow.xaml.cs      # About dialog
│   │   │   ├── SettingsWindow.xaml.cs   # Preferences window
│   │   │   └── AdvancedSettingsWindow.xaml.cs # Advanced settings + hotkey config
│   │   ├── Controls/
│   │   │   ├── HotkeyRecorder.xaml      # HotkeyRecorder UserControl UI
│   │   │   └── HotkeyRecorder.xaml.cs   # Click-to-record hotkey with keycap UI
│   │   └── libgonhanh_core.dll  # Compiled Rust DLL
│   │
│   └── linux/                   # Beta: Fcitx5 addon (~500 LOC)
│       ├── src/
│       │   ├── Engine.h/cpp      # Fcitx5 InputMethodEngine implementation
│       │   ├── RustBridge.h/cpp  # C++ FFI wrapper to Rust core
│       │   └── KeycodeMap.h      # X11/Wayland keysym → keycode mapping
│       ├── data/
│       │   ├── gonhanh-addon.conf # Fcitx5 addon registration
│       │   └── gonhanh.conf      # Input method configuration
│       ├── scripts/
│       │   ├── build.sh          # CMake build script
│       │   └── install.sh        # User-local installation script
│       └── libgonhanh_core.so    # Compiled Rust shared library (x86_64)
│
├── scripts/                     # Build automation
│   ├── setup.sh                # Environment setup (installs Rust, arms cargo-nextest)
│   ├── build-core.sh           # Build universal Rust library (arm64 + x86_64)
│   ├── build-macos.sh          # Build macOS SwiftUI app + DMG
│   ├── build-windows.ps1       # PowerShell build script for Windows
│   └── generate-release-notes.sh # Release notes generator
│
├── Makefile                    # Main build targets
├── .github/workflows/          # CI/CD automation
│   ├── ci.yml                 # Run on push/PR: format, clippy, tests
│   └── release.yml            # Run on tags: build, create GitHub release
│
├── CLAUDE.md                   # Developer guidance (architecture, patterns, commands)
├── README.md                   # Project overview + quick start
└── docs/                       # Documentation (this folder)
```

## Core Module Responsibilities

### Engine Modules (core/src/engine/)

#### `engine/mod.rs` - Main Processing Pipeline
**Lines**: ~500 | **Complexity**: High | **Source**: `core/src/engine/mod.rs`

Central `Engine` struct orchestrating 7-stage keystroke processing:
1. **Stroke detection** (đ/Đ) - Single key transformation
2. **Tone mark detection** (sắc/huyền/hỏi/ngã/nặng) - Multi-key sequences
3. **Vowel mark detection** (circumflex/horn/breve) - Multi-key sequences
4. **Mark removal** (reverse vowel transformation) - Undo previous marks
5. **W-vowel handling** (Telex-specific "w"→"ư") - Context-aware substitution
6. **Normal letter processing** - Regular keystroke
7. **Shortcut expansion** (user-defined) - Abbreviation matching

**Uppercase Detection** (line 718, fixed 2025-12-26):
- `caps || shift` - Uppercase when CapsLock OR Shift is held
- Previous bug: Only checked `caps`, causing Shift+DD → đ instead of Đ
- Now correctly produces uppercase diacritical characters with Shift

**Result**: Returns `Result` struct with action (None/Send/Restore), backspace count, output chars

**Key Functions**:
- `pub fn new() -> Self` - Initialize with empty buffer
- `pub fn process_key(&mut self, key: u16, is_shift: bool) -> Result` - Main entry point
- `pub fn clear(&mut self)` - Reset buffer (word boundary)
- `pub fn set_enabled(&mut self, enabled: bool)` - Toggle on/off
- `pub fn set_method(&mut self, method: InputMethod)` - Switch Telex/VNI
- `pub fn set_shortcuts(&mut self, shortcuts: Vec<Shortcut>)` - User abbreviations

#### `engine/buffer.rs` - Circular Typing Buffer
**Lines**: ~300 | **Complexity**: Medium | **Source**: `core/src/engine/buffer.rs`

Fixed 64-character circular buffer for multi-keystroke context. Tracks tone mark, vowel mark, and stroke for each character. Implements tone/mark repositioning (e.g., "hoaf" → "hoà").

**Key Methods**:
- `append_char(&mut self, ch: char, tone: ToneType, mark: VowelMark)`
- `remove_last(&mut self)` - Backspace operation
- `get_context(&self, count: usize) -> String` - Get last N chars
- `last_vowel_pos(&self) -> Option<usize>` - Find tone anchor point

#### `engine/syllable.rs` - Vietnamese Syllable Parsing
**Lines**: ~400 | **Complexity**: Medium-High | **Source**: `core/src/engine/syllable.rs`

Parses buffer into syllable components: (C₁)(G)V(C₂)
- C₁ = initial consonant
- G = glide (y/w)
- V = vowel
- C₂ = final consonant

Identifies vowel position for tone/mark placement based on linguistic rules.

#### `engine/validation.rs` - Vietnamese Phonology Rules
**Lines**: ~350 | **Complexity**: High | **Source**: `core/src/engine/validation.rs`

**5 Validation Rules** (applied BEFORE transformation, validation-first approach):
1. **Must have vowel**: Every valid syllable contains at least one vowel
2. **Valid initials**: Only 16 single consonants + 10 pairs + ngh allowed at start
3. **All chars parsed**: Every character fits syllable pattern (C+G+V+C)
4. **Spelling rules**: Enforce c/k/g restrictions (c→e/i only, k→non-e/i only, etc.)
5. **Valid finals**: Only c,ch,m,n,ng,nh,p,t allowed at end

#### `engine/transform.rs` - Diacritic & Tone Application
**Lines**: ~600 | **Complexity**: Very High | **Source**: `core/src/engine/transform.rs`

Pattern-based transformation (not case-by-case). Applies tones and vowel marks with special handling:
- **UO Compound**: "duoc" + horn → "dươc" (both u,o receive horn)
- **Tone Repositioning**: Smart placement by vowel combinations
- **Revert Logic**: "ass" (double mark) → "as" (undo, keep mark+key)
- **Last-transform tracking**: For intelligent reverting

#### `engine/shortcut.rs` - User-Defined Abbreviations
**Lines**: ~500 | **Complexity**: Medium | **Source**: `core/src/engine/shortcut.rs`

Priority-based matching system. Supports arbitrary abbreviation → expansion (e.g., "hv" → "không"). Longest-match-first strategy to avoid conflicts.

### Input Method Modules (core/src/input/)

#### `input/telex.rs` - Telex Input Method
**Lines**: ~200 | **Complexity**: Medium | **Source**: `core/src/input/telex.rs`

Vietnamese VIQR-style: a+s → á, a+f → à, a+r → ả, a+x → ã, a+j → ạ

Tone marks: s=sắc, f=huyền, r=hỏi, x=ngã, j=nặng
Vowel marks: w=ư (horn on u), a→â (circumflex), e→ê, o→ô
Special: dd → đ, w alone → ư, nhw → như

#### `input/vni.rs` - VNI Input Method
**Lines**: ~200 | **Complexity**: Medium | **Source**: `core/src/input/vni.rs`

Vietnamese numeric: a+1 → á, a+2 → à, etc.

Tone marks: 1=sắc, 2=huyền, 3=hỏi, 4=ngã, 5=nặng
Vowel marks: 6=circumflex, 7=horn, 8=breve
Stroke: d+9 → đ
Symbol typing: Shift+number skips normal letter, triggers mark directly

### Data Modules (core/src/data/)

#### `data/vowel.rs` - Vowel Transformation Table
**Lines**: ~300 | **Complexity**: Low | **Source**: `core/src/data/vowel.rs`

Pre-computed 72-entry table: 12 base vowels × 6 tone marks
Maps (vowel_char, tone_type) → transformed_char
Supports case preservation (à ↔ À)

#### `data/keys.rs` - Input Method Keycode Mappings
**Source**: `core/src/data/keys.rs`

Maps virtual keycodes to character representation, handles shift/caps lock modifiers.

#### `data/chars.rs` - Character Constants
**Source**: `core/src/data/chars.rs`

Pre-computed UTF-32 codepoints for all Vietnamese characters, used for FFI output.

#### `data/constants.rs` - Vietnamese Phonology Constants
**Source**: `core/src/data/constants.rs`

Valid initial consonants, final consonants, consonant clusters, vowel groups.

### FFI Layer (core/src/lib.rs)

**Lines**: ~300 | **Complexity**: High (unsafe) | **Source**: `core/src/lib.rs`

Exports 6 C ABI functions (thread-safe via Mutex). Critical: Must maintain `#[repr(C)]` struct layout exactly.

**Exported Functions**:
```rust
ime_init()                                                  // Initialize
ime_key(key: u16, is_shift: bool, is_ctrl: bool) -> Result // Process keystroke
ime_key_ext(key, is_shift, is_ctrl, special) -> Result     // Extended version
ime_method(method: u8)                                      // Switch input method (0=Telex, 1=VNI)
ime_enabled(enabled: bool)                                  // Toggle on/off
ime_clear()                                                 // Reset buffer
ime_free(result)                                            // Deallocate Result
```

**Result Struct** (matches Swift exactly):
```rust
#[repr(C)]
pub struct Result {
    pub chars: [u32; 32],    // UTF-32 output (128 bytes)
    pub action: u8,          // 0=None, 1=Send, 2=Restore
    pub backspace: u8,       // Characters to delete
    pub count: u8,           // Valid output chars
    pub _pad: u8,            // Alignment padding
}
```

## Platform-Specific Modules

### macOS Platform (platforms/macos/)

#### `RustBridge.swift` - FFI Bridge (CRITICAL)
**Lines**: ~250 | **Responsibility**: Bridge Rust ↔ Swift | **Source**: `platforms/macos/RustBridge.swift`

Must declare `ImeResult` struct matching Rust `Result` byte-for-byte. Wraps all 6 Rust FFI functions. Handles pointer safety with `defer { ime_free(ptr) }`.

#### `MenuBar.swift` - Status Bar UI
**Lines**: ~350 | **Responsibility**: Main app UI | **Source**: `platforms/macos/MenuBar.swift`

Creates NSStatusBar, manages menu items: Enable/Disable, Input Method, Settings, About, Quit. Handles global Ctrl+Space hotkey.

#### `App.swift` - Application Delegate
**Source**: `platforms/macos/App.swift`

AppDelegate for NSApplication. First-run detection, MenuBarController initialization, accessibility permission checking.

#### Other Swift Files
- `OnboardingView.swift` - Permission setup wizard
- `LaunchAtLogin.swift` - SMAppService integration
- `UpdateManager.swift` - DMG download + mounting
- `UpdateChecker.swift` - GitHub release checking
- `SettingsView.swift`, `AboutView.swift`, `UpdateView.swift` - UI components
- `AppMetadata.swift` - Shared app constants

### Windows Platform (platforms/windows/)

#### `Core/RustBridge.cs` - FFI Bridge
**Source**: `platforms/windows/Core/RustBridge.cs`

P/Invoke signatures matching Rust FFI, UTF-32 interop, memory management.

**Advanced FFI Methods** (11 total):
- `ime_skip_w_shortcut(bool)` - Skip w→ư shortcut in Telex
- `ime_esc_restore(bool)` - ESC key restores raw ASCII
- `ime_free_tone(bool)` - Free tone placement (skip validation)
- `ime_english_auto_restore(bool)` - Auto-restore English words
- `ime_auto_capitalize(bool)` - Auto-capitalize after punctuation
- `ime_clear_all()` - Clear buffer + word history
- `ime_get_buffer(IntPtr, int)` - Get full buffer (UTF-32)
- `ime_restore_word(IntPtr)` - Restore Vietnamese word to buffer
- `ime_add_shortcut(IntPtr, IntPtr)` - Add user shortcut
- `ime_remove_shortcut(IntPtr)` - Remove shortcut
- `ime_clear_shortcuts()` - Clear all shortcuts

#### `Core/KeyboardShortcut.cs` - Hotkey Model
**Source**: `platforms/windows/Core/KeyboardShortcut.cs`

Keyboard shortcut representation with KeyCode + Modifiers (Ctrl/Alt/Shift/Win). Provides Matches() for detection, GetDisplayParts() for UI rendering, and Registry serialization (ToRegistryString/FromRegistryString).

#### `Core/KeyboardHook.cs` - Keyboard Interception + Hotkey Detection
**Source**: `platforms/windows/Core/KeyboardHook.cs`

SetWindowsHookEx for system-wide WH_KEYBOARD_LL hook, WM_KEYDOWN processing. Includes Hotkey property for configurable global hotkey and OnHotkeyTriggered event for Vietnamese/English toggle.

#### `Core/TextSender.cs` - Text Input Simulation
**Source**: `platforms/windows/Core/TextSender.cs`

Sends text to active window using **Unicode injection via SendInput API** with `KEYEVENTF_UNICODE` flag (as of 2025-12-26). Replaced previous clipboard-based method to preserve user's clipboard content and correctly maintain uppercase state.

**Text Injection Strategy**:
- **Direct Unicode injection**: Characters injected as keyboard events (similar to macOS `CGEvent.keyboardSetUnicodeString`)
- **No clipboard pollution**: Previously used `Clipboard.SetText()` + Ctrl+V, now uses direct Unicode
- **Uppercase correctness**: Properly handles Shift state for diacritical characters (Shift+DD → Đ)

**Key Methods**:
- `SendText(string text, int backspaces)` - Main entry point for text replacement
- `SendBackspaces(int count, IntPtr marker)` - Batch backspace injection
- `SendUnicodeText(string text, IntPtr marker)` - Unicode character injection with KEYEVENTF_UNICODE flag

#### `Services/SettingsService.cs` - Registry Persistence
**Source**: `platforms/windows/Services/SettingsService.cs`

Stores user preferences, input method selection, enable/disable state, and global hotkey configuration.

**Advanced Settings Properties** (5 total):
- `SkipWShortcut` - Skip w→ư shortcut in Telex mode (default: false)
- `EscRestore` - ESC key restores raw ASCII input (default: true)
- `FreeTone` - Enable free tone placement without validation (default: false)
- `EnglishAutoRestore` - Auto-restore English words like "text", "expect" (default: false)
- `AutoCapitalize` - Auto-capitalize after sentence-ending punctuation (default: true)

**Hotkey Settings**:
- `ToggleHotkey` - KeyboardShortcut property for global Vietnamese/English toggle (default: Ctrl+Space)
- Persisted to Registry, loaded on startup

**Registry Keys**:
- Path: `HKCU\SOFTWARE\GoNhanh`
- Shortcuts: `HKCU\SOFTWARE\GoNhanh\Shortcuts`
- Hotkey: Stored as `ToggleHotkey` string value

#### `Services/ShortcutsManager.cs` - User Shortcuts Management
**Source**: `platforms/windows/Services/ShortcutsManager.cs`

Manages user-defined shortcuts (abbreviations like "vn" → "Việt Nam"). Persists shortcuts to Registry, syncs with Rust engine via RustBridge FFI.

**Key Features**:
- Load/Save shortcuts from/to Registry
- Add, Remove, Clear shortcuts
- Default Vietnamese abbreviations (vn, hn, hcm, ko, dc, vs, ms, etc.)
- Auto-sync with Rust engine on every change

#### `Views/TrayIcon.cs` - System Tray UI
**Source**: `platforms/windows/Views/TrayIcon.cs`

NotifyIcon creation, context menu: Enable/Disable, Input Method, Settings, About.

#### `Views/OnboardingWindow.xaml.cs` - Setup Wizard
**Source**: `platforms/windows/Views/OnboardingWindow.xaml.cs`

Multi-page setup wizard with AutoStart checkbox on Page 3 for enabling Windows auto-start feature during initial setup.

#### `Views/AdvancedSettingsWindow.xaml.cs` - Advanced Features UI
**Source**: `platforms/windows/Views/AdvancedSettingsWindow.xaml.cs`

WPF window for configuring 5 advanced features + global hotkey:
- AutoStart toggle (Windows auto-start on login)
- Skip W Shortcut (Telex mode w→ư behavior)
- ESC Restore (ESC key restores raw ASCII)
- Free Tone (disable validation, place diacritics anywhere)
- English Auto-Restore (auto-detect and restore English words)
- Auto-Capitalize (capitalize after . ! ? Enter)
- Global Hotkey configuration (HotkeyRecorder control)

#### `Controls/HotkeyRecorder.xaml.cs` - Hotkey Recording Control
**Source**: `platforms/windows/Controls/HotkeyRecorder.xaml.cs`

Custom WPF UserControl for recording keyboard shortcuts. Features:
- Click-to-record interaction (focus-based recording)
- Keycap-style UI display (visual keyboard key representation)
- System shortcut conflict detection (blocks Ctrl+C/V/X/A/Z/Y, Alt+Tab/F4)
- Validates against reserved Windows shortcuts
- Exposes Hotkey property for data binding

### Linux Platform (platforms/linux/)

#### `src/Engine.h/cpp` - Fcitx5 Integration
**Lines**: ~200 | **Responsibility**: Input method engine | **Source**: `platforms/linux/src/Engine.h/cpp`

Implements Fcitx5 `InputMethodEngine` interface. Handles input method registration, key processing, and candidate list management.

#### `src/RustBridge.h/cpp` - C++ FFI Wrapper
**Lines**: ~150 | **Responsibility**: Bridge C++ ↔ Rust | **Source**: `platforms/linux/src/RustBridge.h/cpp`

C++ wrapper around Rust FFI, handles UTF-32 conversion and memory safety.

#### `src/KeycodeMap.h` - Keycode Mapping
**Source**: `platforms/linux/src/KeycodeMap.h`

Maps X11/Wayland keysyms to internal keycode representation for compatibility with macOS keycode space.

## Test Coverage

### Test Files (core/tests/)

| File | Purpose | Test Count | Source |
|------|---------|-----------|--------|
| `unit_test.rs` | Module unit tests | ~30 | `core/tests/unit_test.rs` |
| `typing_test.rs` | Full keystroke sequences | ~60 | `core/tests/typing_test.rs` |
| `engine_test.rs` | Engine initialization + state | ~20 | `core/tests/engine_test.rs` |
| `integration_test.rs` | End-to-end keystroke→output | ~35 | `core/tests/integration_test.rs` |
| `paragraph_test.rs` | Multi-word paragraphs | ~15 | `core/tests/paragraph_test.rs` |

**Total**: 160+ test cases, 2100+ lines of test code

**Test Utilities** (core/tests/common/mod.rs):
- `ImeHelper` struct for convenient test setup
- `assert_output()` macro for comparing expected results
- Test data constants

## Entry Points for Common Development Tasks

### Adding a New Input Method
1. Create `core/src/input/mynewmethod.rs` implementing InputMethod trait
2. Export from `core/src/input/mod.rs`
3. Add pattern matching in `engine/mod.rs` process_key()
4. Add test cases in `core/tests/typing_test.rs`
5. Update UI in `platforms/macos/SettingsView.swift`

### Fixing a Transform Bug
1. Locate issue in `core/src/engine/transform.rs`
2. Check `core/src/data/vowel.rs` for vowel table correctness
3. Add failing test case in `core/tests/unit_test.rs` or `typing_test.rs`
4. Fix transform logic
5. Run `make test` to verify

### Optimizing Engine Performance
1. Profile with `cargo flamegraph` (optional dependency)
2. Most time spent in: transform (60%), validation (25%), buffer operations (15%)
3. Avoid allocations in `ime_key()` path (uses stack-allocated arrays)

### Adding Shortcut Support UI
1. Design shortcut edit dialog in `platforms/macos/ShortcutsView.swift`
2. Store in UserDefaults as JSON
3. Parse in `RustBridge.swift` and call `ime_set_shortcuts()`
4. Test with `core/tests/shortcut_test.rs`

### Cross-Platform Port (Windows/Linux)
1. **Core** (core/src/): Already platform-agnostic ✓
2. **Platform wrapper**:
   - Windows: `platforms/windows/` (WPF + P/Invoke) - DONE
   - Linux: `platforms/linux/` (Qt + FFI) - TODO
3. **Build automation**: Add scripts pattern
4. **Testing**: Adapt platform-specific tests

## Module Dependency Graph

```
lib.rs (FFI boundary)
  ↓
engine/mod.rs (orchestration)
  ├─→ engine/buffer.rs
  ├─→ engine/syllable.rs
  ├─→ engine/validation.rs
  ├─→ engine/transform.rs
  │    └─→ data/vowel.rs
  ├─→ engine/shortcut.rs
  ├─→ input/telex.rs (or vni.rs)
  │    └─→ data/keys.rs
  └─→ data/constants.rs

RustBridge.swift (macOS)
  ├─→ lib.rs exports
  └─→ CGEventTap keyboard hook

RustBridge.cs (Windows)
  ├─→ libgonhanh_core.dll exports
  └─→ SetWindowsHookEx keyboard hook
```

## Module Characteristics

| Module | LOC | Responsibility | Stability | Complexity |
|--------|-----|-----------------|-----------|------------|
| engine/mod.rs | 500 | Orchestration | High | High |
| engine/transform.rs | 600 | Diacritics | High | Very High |
| engine/shortcut.rs | 500 | User abbreviations | Medium | Medium |
| engine/validation.rs | 350 | Phonology rules | High | High |
| engine/buffer.rs | 300 | Circular buffer | High | Medium |
| engine/syllable.rs | 400 | Parsing | High | Medium-High |
| input/telex.rs | 200 | Input method | High | Medium |
| input/vni.rs | 200 | Input method | High | Medium |
| data/vowel.rs | 300 | Lookup table | Very High | Low |
| data/* | 550 | Constants | Very High | Low |
| lib.rs | 300 | FFI | Very High | High |
| RustBridge.swift | 250 | FFI Bridge | High | High |
| MenuBar.swift | 350 | UI | Medium | Medium |
| Other Swift | 1100 | Platform | Medium | Low-Medium |

## Performance Characteristics

### Critical Path (ime_key execution)
1. Lock ENGINE mutex (1-2μs)
2. Validate keystroke (100-150μs) - main bottleneck
3. Process transform (50-100μs)
4. Allocate + populate Result (20-30μs)
5. Unlock mutex (1-2μs)

**Total**: 170-285μs (0.17-0.28ms) - well under 1ms budget

### Memory Usage
- Static data: ~150KB (vowel table, keycodes, constants)
- ENGINE global: ~500B (struct only)
- Per keystroke: Stack-allocated arrays only (no heap)
- SwiftUI overhead: ~4.5MB (standard)

**Total app**: ~5MB resident

---

**Last Updated**: 2025-12-26
**Total Files**: 127+ files (includes new HotkeyRecorder control)
**Total Tokens**: ~370,000+ tokens (estimated)
**Total Characters**: ~1,400,000+ chars (estimated)
**Coverage**: 100% of directories documented
**Platforms**: macOS (v1.0.89+), Windows (production, feature parity + hotkey toggle), Linux (beta)

**Windows Platform Recent Updates (2025-12-26)**:
- ✅ **Unicode text injection** - Replaced clipboard-based injection (TextSender.cs, preserves clipboard)
- ✅ **Uppercase fix** - Engine checks CapsLock OR Shift (mod.rs:718, Shift+DD → Đ works correctly)
- ✅ Global hotkey toggle (Ctrl+Space default, configurable)
- ✅ HotkeyRecorder UserControl with keycap-style UI
- ✅ KeyboardShortcut model with Registry serialization
- ✅ AutoStart UI in OnboardingWindow (Page 3)
- ✅ Hotkey configuration in AdvancedSettingsWindow
- ✅ KeyboardHook integration with OnHotkeyTriggered event
- ✅ System shortcut conflict detection

**Windows Platform Complete Features**:
- ✅ 5 Advanced Settings (SkipWShortcut, EscRestore, FreeTone, EnglishAutoRestore, AutoCapitalize)
- ✅ Shortcuts Manager with Registry persistence
- ✅ Advanced Settings UI window
- ✅ 11 RustBridge FFI methods
- ✅ Global hotkey toggle
- ✅ Auto-start configuration
- ✅ Unicode text injection (clipboard-safe)
- ✅ Full feature compatibility with macOS version

## Top 5 Largest Files by Token Count

| Rank | File | Tokens | Chars | % of Total |
|------|------|--------|-------|------------|
| 1 | `core/src/engine/mod.rs` | 44,881 | 201,327 | 12.2% |
| 2 | `core/tests/integration_test.rs` | 29,530 | 106,191 | 8.0% |
| 3 | `docs/vietnamese-language-system.md` | 21,139 | 56,183 | 5.8% |
| 4 | `core/tests/typing_test.rs` | 20,164 | 59,308 | 5.5% |
| 5 | `platforms/macos/RustBridge.swift` | 14,421 | 59,869 | 3.9% |
