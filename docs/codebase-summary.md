# FKey Vietnamese IME — Codebase Summary

> **Last updated:** 2026-08-20

## 1. Project Overview

FKey is a Vietnamese Input Method Editor (IME) for Windows, built as a two-layer system: a **Rust core engine** handling Vietnamese phonology, diacritics, tone placement, and spelling validation, and a **Go/Wails v3 Windows app** providing the system tray UI, low-level keyboard hook, and text injection via Win32 APIs. The entire application ships as a single ~11 MB executable with the Rust DLL embedded, zero runtime dependencies on the Rust side, and minimal Go dependencies.

---

## 2. Directory Structure

```
fkey/
├── core/                              # Rust core engine (Vietnamese IME logic)
│   ├── src/
│   │   ├── lib.rs                     # FFI C-ABI exports (~916 lines)
│   │   ├── utils.rs                   # String/char utilities (~522 lines)
│   │   ├── engine/
│   │   │   ├── mod.rs                 # Engine struct, on_key pipeline (~2.0k lines)
│   │   │   ├── auto_restore.rs        # English auto-restore logic (~3.2k lines)
│   │   │   ├── modifiers.rs           # Tone/mark/stroke/remove dispatch (~2.5k lines)
│   │   │   ├── tests_inline.rs        # Engine unit tests (~494 lines)
│   │   │   ├── shortcut_flow.rs       # Word-boundary shortcut trigger (~57 lines)
│   │   │   ├── rebuild.rs             # Buffer-range → output rendering (~70 lines)
│   │   │   ├── helpers.rs             # Small shared free-function helpers (~53 lines)
│   │   │   ├── capitalize.rs          # Auto-capitalize helpers (~52 lines)
│   │   │   ├── buffer.rs              # Keystroke buffer management
│   │   │   ├── shortcut.rs            # User-defined abbreviations (~966 lines)
│   │   │   ├── syllable.rs            # Vietnamese syllable parsing
│   │   │   ├── transform.rs           # Diacritic/tone transformation
│   │   │   └── validation.rs          # Vietnamese spelling validation (~676 lines)
│   │   ├── data/
│   │   │   ├── chars.rs               # Vietnamese character maps (mark/tone)
│   │   │   ├── vowel.rs               # Vowel phonology tables (~824 lines)
│   │   │   ├── keys.rs                # macOS keycode constants
│   │   │   ├── english_dict.rs        # 100k English word dictionary
│   │   │   ├── telex_doubles.rs       # Telex double-key patterns (~10k lines)
│   │   │   ├── dictionary.rs          # Vietnamese word validation (HashSet, ~0.5MB)
│   │   │   ├── constants.rs           # Shared constants
│   │   │   └── dictionaries/          # Dictionary files (vi.dic, keep.dic)
│   │   ├── input/
│   │   │   ├── mod.rs                 # Input method trait/types
│   │   │   ├── telex.rs               # Telex input method
│   │   │   └── vni.rs                 # VNI input method
│   │   └── updater/
│   │       └── mod.rs                 # Version parsing
│   ├── tests/                         # 27 test files, ~15k lines
│   └── Cargo.toml                     # Zero runtime dependencies
│
├── platforms/
│   └── windows-wails/                 # Windows app (Go + Wails v3)
│       ├── main.go                    # App entry, tray menu, init
│       ├── updater_ui.go              # Update-check background task + install dialog flow
│       ├── bindings.go                # Frontend ↔ Go bindings
│       ├── core/                      # Go wrapper for Rust DLL + Win32
│       │   ├── smart_paste_test.go    # Mojibake fix unit tests
│       │   ├── keyboard_hook_test.go  # Buffer-clear key class + consumed-map tests
│       │   ├── hook_pump_test.go      # Dedicated hook thread lifecycle tests
│       │   ├── bridge.go              # Rust DLL FFI bridge, VK→mac lookup table, typed FFI struct
│       │   ├── keyboard_hook.go       # Win32 low-level keyboard hook
│       │   ├── mouse_hook.go          # Win32 low-level mouse hook (buffer clear on click)
│       │   ├── hook_pump.go           # Dedicated OS thread + message pump for both hooks
│       │   ├── format_hotkey_router.go # 3-tier format-hotkey routing
│       │   ├── startup_trace.go       # Per-stage boot timing log
│       │   ├── debug_trace.go         # FKEY_DEBUG digit-path logging
│       │   ├── ime_loop.go            # IME processing pipeline
│       │   ├── text_sender.go         # SendInput Unicode text injection
│       │   ├── app_detector.go        # App detection, injection profiles
│       │   ├── coalescer.go           # Keystroke coalescing
│       │   ├── smart_paste.go         # Mojibake fix via Ctrl+Shift+V
│       │   ├── elevation.go           # UAC elevation/de-elevation
│       │   ├── clipboard.go           # Clipboard operations
│       │   ├── format_handler.go      # Unicode text formatting
│       │   └── format_hotkeys.go      # Format hotkey detection
│       ├── services/
│       │   ├── settings.go            # Registry-based settings (HKCU\SOFTWARE\FKey)
│       │   ├── updater.go             # GitHub auto-updater
│       │   └── formatting.go          # Formatting config (JSON)
│       ├── frontend/                  # WebView2 UI (HTML/JS/CSS)
│       ├── tests/
│       │   └── fkey_test.go           # Go unit tests
│       ├── build.ps1                  # PowerShell build script
│       ├── dll_embed.go               # Embeds Rust DLL into Go binary
│       ├── icons.go                   # Runtime tray icon generation
│       └── go.mod                     # Go 1.25, Wails v3 alpha.60
│
├── assets/                            # Logo, banner images
├── docs/                              # Documentation
├── VERSION                            # Current version
├── AGENTS.md                          # Agent instructions
└── README.md                          # User-facing README (Vietnamese)
```

---

## 3. Module Map

### Rust Core (`core/`)

| Module | Purpose | Key Types / Functions |
|--------|---------|----------------------|
| `lib.rs` | C-ABI FFI boundary | `process_key()`, `create_engine()`, `destroy_engine()` — exports consumed by Go via DLL |
| `utils.rs` | String/char helpers | Unicode normalization, char classification, tone/mark detection |
| **engine/** | | |
| `engine/mod.rs` | Central Engine struct | `Engine`, `on_key`/`on_key_ext` — main keystroke pipeline; delegates to the child modules below |
| `engine/auto_restore.rs` | English auto-restore | `should_auto_restore`, `try_auto_restore_on_space/on_break`, `restore_to_raw`, `restore_word` |
| `engine/modifiers.rs` | Modifier dispatch | `try_stroke`, `try_tone`, `try_mark`, `try_remove`, `try_w_as_vowel`, `try_bracket_as_vowel` |
| `engine/tests_inline.rs` | Engine unit tests | Telex/VNI basic + ESC-restore test tables, moved out of `mod.rs` |
| `engine/shortcut_flow.rs` | Shortcut trigger | `try_word_boundary_shortcut[_with_char]` |
| `engine/rebuild.rs` | Output rendering | `rebuild_from`, `rebuild_from_after_insert` |
| `engine/helpers.rs` | Shared free functions | `break_key_to_char` |
| `engine/capitalize.rs` | Auto-capitalize | `is_sentence_ending_punctuation`, `should_reset_pending_capitalize`, `set_auto_capitalize` |
| `engine/buffer.rs` | Keystroke buffer | Tracks raw input, composed output, cursor position |
| `engine/shortcut.rs` | Abbreviation expansion | User-defined shortcuts, trigger condition matching |
| `engine/syllable.rs` | Syllable parsing | Splits Vietnamese words into onset/nucleus/coda/tone components |
| `engine/transform.rs` | Diacritic/tone ops | Applies/removes marks (ă, ơ, ê…) and tones (sắc, huyền, hỏi, ngã, nặng) |
| `engine/validation.rs` | Spelling rules | Validates Vietnamese syllable structure, consonant clusters, vowel combos |
| **data/** | | |
| `data/chars.rs` | Character maps | Mark → base char mappings, tone → char mappings |
| `data/vowel.rs` | Vowel phonology | Vowel combination tables, tone placement rules per vowel cluster |
| `data/english_dict.rs` | English dictionary | ~100k words for English auto-restore detection |
| `data/dictionary.rs` | Vietnamese dictionary | HashSet-based word validation (~0.5MB), keep list for auto-restore exceptions |
| `data/telex_doubles.rs` | Telex patterns | Double-key reversal patterns (e.g., `aa` → `â` → `aa`) |
| **input/** | | |
| `input/mod.rs` | Input method trait | `InputMethod` trait definition |
| `input/telex.rs` | Telex method | Maps Telex keystrokes (s→sắc, f→huyền, w→ư/ơ, etc.) |
| `input/vni.rs` | VNI method | Maps VNI number keystrokes (1→sắc, 2→huyền, etc.) |

### Go/Wails Platform (`platforms/windows-wails/`)

| Module | Purpose | Key Types / Functions |
|--------|---------|----------------------|
| `main.go` | App entry | Wails app init, system tray menu, window management |
| `updater_ui.go` | Updater dialog flow | `checkForUpdatesBackground`, `performAutoUpdate` — split out of `main.go` |
| `bindings.go` | JS ↔ Go bridge | `SettingsService`, `FormattingService` — methods callable from frontend |
| **core/** | | |
| `core/bridge.go` | Rust FFI | `ProcessKey()`, `NewEngine()`, `TranslateToMacKeycode()` (table-driven) — Go wrappers around `gonhanh_core.dll` |
| `core/keyboard_hook.go` | Keyboard hook | Win32 `SetWindowsHookEx(WH_KEYBOARD_LL)`, key event dispatch, panic recovery, `goSafe()` helper, `IsBufferClearKey()` nav/F-key/numpad class |
| `core/mouse_hook.go` | Mouse hook | Win32 `SetWindowsHookEx(WH_MOUSE_LL)` — clears IME buffer on button-down |
| `core/hook_pump.go` | Dedicated hook thread | `runtime.LockOSThread()` + `GetMessageW` pump so the hooks are serviced independently of Wails' main thread |
| `core/format_hotkey_router.go` | Format hotkey routing | `routeFormatHotkey()` — custom/global/default 3-tier matching |
| `core/startup_trace.go` | Boot timing | Appends per-stage timing to `startup.log` |
| `core/debug_trace.go` | Digit-path debug log | `FKEY_DEBUG=1`-gated, ring buffer of recent buffer-clear-class keys |
| `core/ime_loop.go` | IME pipeline | Goroutine processing keystroke → engine → text output, smart profile cache invalidation |
| `core/text_sender.go` | Text injection | `SendInput()` Unicode injection, backspace simulation |
| `core/app_detector.go` | App profiles | Detects foreground app, selects injection strategy, window-aware smart profile cache (`GetSmartAppProfile()`) |
| `core/coalescer.go` | Coalescing | Batches rapid keystrokes for apps like Discord |
| `core/smart_paste.go` | Mojibake fix | Detects and fixes UTF-8 → CP1252 mojibake via clipboard |
| `core/elevation.go` | UAC handling | Elevates/de-elevates process for admin app input |
| `core/clipboard.go` | Clipboard | Read/write clipboard for paste-based injection |
| `core/format_handler.go` | Text formatting | Unicode bold/italic/strikethrough transforms |
| `core/format_hotkeys.go` | Format hotkeys | Ctrl+B/I/U/S hotkey detection |
| **services/** | | |
| `services/settings.go` | Settings | Read/write `HKCU\SOFTWARE\FKey` registry keys |
| `services/updater.go` | Auto-update | Checks GitHub Releases, downloads `.exe`, applies update |
| `services/formatting.go` | Format config | Loads/saves `formatting.json` |

---

## 4. Key Files Quick Reference

### Core Algorithm Files (start here to understand the engine)

| File | Why It Matters |
|------|----------------|
| `core/src/engine/mod.rs` | **The heart** — `Engine` struct and its `on_key` pipeline (~2k lines); delegates tone/mark logic, auto-restore, and rebuild to the child modules listed in the module map above |
| `core/src/engine/auto_restore.rs` | English auto-restore and undo-to-raw-ASCII logic (largest single file in the engine) |
| `core/src/engine/modifiers.rs` | Tone/mark/stroke modifier dispatch — where a keystroke becomes a diacritic |
| `core/src/engine/transform.rs` | How diacritics and tones are applied to characters |
| `core/src/engine/validation.rs` | Vietnamese spelling rules that determine valid output |
| `core/src/engine/syllable.rs` | How input is parsed into Vietnamese syllable components |
| `core/src/data/vowel.rs` | Vowel phonology tables driving tone placement |
| `core/src/input/telex.rs` | Telex key mappings (the most popular input method) |

### Platform Integration Files

| File | Why It Matters |
|------|----------------|
| `platforms/windows-wails/core/bridge.go` | FFI boundary between Go and Rust DLL |
| `platforms/windows-wails/core/keyboard_hook.go` | How keystrokes are intercepted at OS level |
| `platforms/windows-wails/core/text_sender.go` | How Vietnamese text is injected into target apps |
| `platforms/windows-wails/core/app_detector.go` | App-specific injection strategies (terminals, browsers, etc.) |
| `platforms/windows-wails/main.go` | App lifecycle, tray menu, window creation |

### Test Files

| File | Coverage Area |
|------|---------------|
| `core/tests/integration_test.rs` | Full typing sequences (~3697 lines) |
| `core/tests/bug_reports_test.rs` | Regression tests from user reports (~1923 lines) |
| `core/tests/english_auto_restore_test.rs` | English word detection (~1421 lines) |
| `core/tests/digit_bug_fixes_test.rs` | VNI/Telex digit-modifier regression coverage (markable-context gate, shift+digit, o2o) |
| `core/tests/typing_test.rs` | Keystroke-by-keystroke typing simulation |
| `platforms/windows-wails/tests/fkey_test.go` | Go unit tests (31 tests) |
| `platforms/windows-wails/core/smart_paste_test.go` | Mojibake detection/fix unit tests (3 tests) |
| `platforms/windows-wails/core/keyboard_hook_test.go` | Buffer-clear key class + consumed-map reset tests (2 tests) |
| `platforms/windows-wails/core/hook_pump_test.go` | Dedicated hook thread start/stop lifecycle tests (3 tests) |

---

## 5. Stats

| Metric | Value |
|--------|-------|
| Rust core `src/` LOC | ~24.9k (engine/ ~11.0k across mod.rs + 7 child modules, data tables ~14k) |
| Go platform LOC (excl. tests) | ~7.0k |
| Rust test LOC | ~20k |
| Go test LOC | ~1.1k |
| Rust test files | 27 test binaries + 1 shared `common/mod.rs` helper |
| Go test files | 4 (`tests/fkey_test.go`, `core/smart_paste_test.go`, `core/keyboard_hook_test.go`, `core/hook_pump_test.go`) |
| Go test count | 39 (31 + 3 + 2 + 3) |
| Runtime dependencies (Rust) | 0 |
| Runtime dependencies (Go) | Wails v3, golang.org/x/sys |
| Final binary size | ~11 MB (single .exe, DLL embedded) |
| Current version | See `VERSION` file |

---

## 6. Technology Stack

| Layer | Technology | Version | Purpose |
|-------|-----------|---------|---------|
| Core engine | Rust | stable | Vietnamese IME logic, phonology, validation |
| DLL interface | C-ABI FFI | — | Rust ↔ Go boundary |
| Windows app | Go | 1.25 | System integration, keyboard hook, text injection |
| UI framework | Wails | v3 alpha.60 | WebView2 wrapper, system tray, bindings |
| Frontend | HTML/CSS/JS | — | Settings UI in WebView2 |
| Keyboard hook | Win32 API | — | `SetWindowsHookEx(WH_KEYBOARD_LL)` |
| Text injection | Win32 API | — | `SendInput()` Unicode events |
| Settings | Windows Registry | — | `HKCU\SOFTWARE\FKey` |
| Build | PowerShell | — | `build.ps1` orchestrates Rust + Go builds |
| Package | Single .exe | — | DLL embedded via `go:embed` |

---

## 7. Data Flow

```
Keystroke (Win32 hook)
  → keyboard_hook.go (intercept, with panic recovery)
  → ime_loop.go (dispatch, invalidate caches on app switch)
  → bridge.go (FFI call)
  → lib.rs → Engine::process_key()
      → input/telex.rs or vni.rs (map key)
      → engine/transform.rs (apply mark/tone)
      → engine/validation.rs (check spelling)
      → engine/syllable.rs (parse syllable)
  ← EngineResult { committed_text, buffer_display, backspaces }
  → GetSmartAppProfile() (cached per window handle)
  → text_sender.go (SendInput or clipboard inject)
  → Target application receives Vietnamese text
```

---

## 8. Settings Storage

All settings stored in Windows Registry at `HKEY_CURRENT_USER\SOFTWARE\FKey`:

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| InputMethod | DWORD | 0 | 0=Telex, 1=VNI |
| ModernTone | DWORD | 1 | Modern tone placement |
| Enabled | DWORD | 1 | IME active |
| AutoStart | DWORD | 0 | Start with Windows |
| SkipWShortcut | DWORD | 0 | Skip w→ư in Telex |
| EscRestore | DWORD | 1 | ESC restores raw input |
| FreeTone | DWORD | 0 | Free tone placement |
| EnglishAutoRestore | DWORD | 0 | Auto-restore English words |
| AutoCapitalize | DWORD | 0 | Auto-capitalize |
| ToggleHotkey | String | "0,5" | Hotkey (keycode,modifiers) |
| CoalescingApps | String | "discord,..." | Apps using coalesced injection |
| ShowOSD | DWORD | 0 | OSD on toggle |
| SmartPaste | DWORD | 1 | Mojibake auto-fix |
| RunAsAdmin | DWORD | 0 | Admin privileges |
| FirstRun | DWORD | 1 | First-run onboarding flag (cleared after setup) |

---

## 9. Build & Test Commands

```bash
# Rust tests
powershell.exe -Command "cd 'D:\WORKSPACES\PERSONAL\fkey\core'; cargo test 2>&1"

# Rust DLL build
powershell.exe -Command "cd 'D:\WORKSPACES\PERSONAL\fkey\core'; cargo build --release 2>&1"

# Go tests
powershell.exe -Command "cd 'D:\WORKSPACES\PERSONAL\fkey\platforms\windows-wails'; go test ./... 2>&1"

# Full Windows build (dev)
powershell.exe -Command "cd 'D:\WORKSPACES\PERSONAL\fkey\platforms\windows-wails'; .\build.ps1 2>&1"

# Full Windows build (release)
powershell.exe -Command "cd 'D:\WORKSPACES\PERSONAL\fkey\platforms\windows-wails'; .\build.ps1 -Release 2>&1"
```
