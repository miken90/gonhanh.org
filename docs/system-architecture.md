# System Architecture

## Overview

FKey is a Vietnamese Input Method Editor (IME) for Windows with a two-layer architecture: a **Rust core engine** that handles Vietnamese text processing, and a **Go/Wails v3 platform layer** that integrates with Windows via Win32 APIs. The layers communicate through a C-ABI FFI bridge—no CGo required.

---

## Architecture Layers

```
┌─────────────────────────────────────────────────────┐
│                   WebView2 Frontend                  │
│                 (HTML / CSS / JS)                     │
├─────────────────────────────────────────────────────┤
│              Wails v3 Bindings (bindings.go)         │
├──────────┬──────────────────────────────┬───────────┤
│ Services │         Core (Go)            │  main.go  │
│----------│------------------------------│-----------|
│ settings │  ime_loop    keyboard_hook   │  tray UI  │
│ updater  │  text_sender app_detector    │  menus    │
│ format   │  smart_paste coalescer       │  events   │
│          │  elevation   clipboard       │           │
├──────────┴──────────┬───────────────────┴───────────┤
│                     │  FFI Bridge (bridge.go)        │
│                     │  syscall.LoadDLL → ime_*()     │
├─────────────────────┴───────────────────────────────┤
│              Rust Core Engine (gonhanh_core.dll)      │
│  ┌──────────┬──────────┬──────────┬──────────┐      │
│  │  engine/  │  data/   │  input/  │  utils   │      │
│  │  buffer   │  chars   │  telex   │          │      │
│  │  shortcut │  vowel   │  vni     │          │      │
│  │  syllable │  keys    │          │          │      │
│  │  transform│  english │          │          │      │
│  │  validate │  dict    │          │          │      │
│  │           │  dictny  │          │          │      │
│  └──────────┴──────────┴──────────┴──────────┘      │
└─────────────────────────────────────────────────────┘
```

---

## Rust Core Engine (`core/`)

The engine is a pure Rust library with **zero runtime dependencies**. It compiles to a DLL (`cdylib`) and exposes a C-ABI FFI interface.

### Engine Architecture

**Validation-first, pattern-based** approach:

1. Keystroke enters buffer
2. Engine scans entire buffer for Vietnamese patterns (not case-by-case)
3. Validates against Vietnamese spelling rules before applying transforms
4. Applies diacritic marks and tones using longest-match-first strategy
5. Returns `Result` struct with action (None/Send/Restore), backspaces, and replacement chars

### Key Modules

`engine/mod.rs` holds only the `Engine` struct and its `on_key`/`on_key_ext` processing pipeline (~2k lines); everything else that used to live in one file is split into focused child modules. Child modules can read `Engine`'s private fields (Rust visibility flows down the module tree), so the split is purely organizational — no field or behavior changes.

| Module | File | Purpose |
|--------|------|---------|
| **Engine** | `engine/mod.rs` | `Engine` struct, `on_key`/`on_key_ext` dispatch, phonology-query helpers (`collect_vowels`, `has_final_consonant`, etc.) |
| **Capitalize** | `engine/capitalize.rs` | Sentence-ending-punctuation detection, `pending_capitalize` reset rules, `set_auto_capitalize` |
| **Helpers** | `engine/helpers.rs` | Small free-function helpers shared across the pipeline (e.g. break-key → character) |
| **Shortcut flow** | `engine/shortcut_flow.rs` | Word-boundary shortcut trigger logic (space/punctuation-triggered expansion) |
| **Auto-restore** | `engine/auto_restore.rs` | English auto-restore: detects when a transformed buffer is actually an English word typed through and restores the raw ASCII (largest child module — the bulk of the file's logic) |
| **Modifiers** | `engine/modifiers.rs` | Tone/mark/stroke/remove modifier dispatch (`try_stroke`, `try_tone`, `try_mark`, `try_remove`, `try_w_as_vowel`, `try_bracket_as_vowel`) and their gate/revert helpers |
| **Rebuild** | `engine/rebuild.rs` | Renders a buffer range back to backspace-count + replacement chars for the platform layer |
| **Buffer** | `engine/buffer.rs` | Fixed-size keystroke buffer (`MAX=256`), no heap alloc per keystroke |
| **Shortcut** | `engine/shortcut.rs` | User abbreviations with trigger conditions (Immediate/OnWordBoundary) |
| **Syllable** | `engine/syllable.rs` | Vietnamese syllable parsing and decomposition |
| **Transform** | `engine/transform.rs` | Diacritic/tone placement and transformation |
| **Validation** | `engine/validation.rs` | Vietnamese spelling validation, foreign word detection |
| **Chars** | `data/chars.rs` | Character maps for marks (ă, â, ê, ô, ơ, ư, đ) and tones |
| **Vowel** | `data/vowel.rs` | Vowel phonology tables for tone placement rules |
| **Input** | `input/telex.rs`, `input/vni.rs` | Input method keystroke-to-diacritic mappings |
| **Dictionary** | `data/dictionary.rs` | Vietnamese word validation via HashSet (~0.5MB), keep list |
| **English Dict** | `data/english_dict.rs` | 100k English words for auto-restore feature |

**VNI digit modifier gating**: a VNI digit (1-9, 0 — mark/tone/stroke/remove are all digit-driven in VNI) only acts as a modifier when the buffer's last character is a letter, i.e. it's continuing an in-progress syllable. If the buffer is empty or already ends in a digit/symbol, the digit passes through as a literal instead of hunting an earlier vowel elsewhere in the buffer. This prevents plain numeric input (or a stale buffer left over from an untracked key on the Go side) from being misread as a tone/mark command. Intentional modifiers like `a1` (á) or `o6` (ô) are unaffected, since the buffer always ends in the letter just typed at the moment the digit arrives.

### FFI Interface (`lib.rs`)

All exports use `#[no_mangle] pub extern "C" fn`:

| Function | Purpose |
|----------|---------|
| `ime_init()` | Initialize engine (call once) |
| `ime_key(key, caps, ctrl)` | Process keystroke |
| `ime_key_ext(key, caps, ctrl, shift)` | Process with shift info |
| `ime_key_with_char(key, caps, ctrl, shift, char_code)` | Process with actual Unicode char |
| `ime_method(method)` | Set input method (0=Telex, 1=VNI) |
| `ime_enabled(enabled)` | Enable/disable processing |
| `ime_clear()` | Clear buffer on word boundary |
| `ime_modern(modern)` | Toggle modern tone placement |
| `ime_free_tone(enabled)` | Toggle free tone mode |
| `ime_esc_restore(enabled)` | Toggle ESC restore |
| `ime_english_auto_restore(enabled)` | Toggle English auto-restore |
| `ime_auto_capitalize(enabled)` | Toggle auto-capitalize |
| `ime_add_shortcut(trigger, replacement)` | Add text shortcut |
| `ime_remove_shortcut(trigger)` | Remove shortcut |
| `ime_restore_word(word)` | Restore word to buffer for editing |
| `ime_get_buffer()` | Get current buffer contents |
| `ime_free(ptr)` | Free result memory |

### Result Struct

```c
struct Result {
    uint32 chars[256];  // UTF-32 codepoints
    uint8  action;      // 0=None, 1=Send, 2=Restore
    uint8  backspace;   // chars to delete
    uint8  count;       // valid chars in array
    uint8  flags;       // bit 0: key_consumed
};
```

On the Go side, `bridge.go` mirrors this exact layout as a typed `cResult` struct (`chars [256]uint32; action, backspace, count, flags uint8`) cast once via `unsafe.Pointer`, with a compile-time size assertion pinning it to 1028 bytes (256×4 + 4) — instead of manually indexing into a raw `[1028]byte`. `flags` is present for layout correctness but not currently read by the Go side.

---

## Windows Platform (`platforms/windows-wails/`)

Go application using Wails v3 framework with WebView2 for the settings UI.

### Core Components

| Component | File | Purpose |
|-----------|------|---------|
| **Bridge** | `core/bridge.go` | FFI to Rust DLL via `syscall.LoadDLL`. Translates Windows VK → macOS keycodes via a `[256]uint16` lookup table built once at package init |
| **Keyboard Hook** | `core/keyboard_hook.go` | Win32 `WH_KEYBOARD_LL` low-level hook. System-wide keystroke interception. Panic recovery prevents crashes under resource pressure. Buffer-clear key class (nav/PgUp-PgDn/Home-End/Insert/Delete/F-keys/numpad) and a held-Win-key check both clear the IME buffer and pass the key through, since neither class ever reaches the engine otherwise |
| **Mouse Hook** | `core/mouse_hook.go` | Win32 `WH_MOUSE_LL` low-level hook. Clears the IME buffer on left/right/middle button-down, since a click can move the caret without any key event |
| **Hook Pump** | `core/hook_pump.go` | Runs the keyboard and mouse hooks on a dedicated, locked OS thread with its own `GetMessageW` message pump — see "Dedicated Hook Thread" below |
| **Format Hotkey Router** | `core/format_hotkey_router.go` | 3-tier format-hotkey routing (custom per-app → global custom → default) called from the keyboard hook |
| **Startup Trace** | `core/startup_trace.go` | Per-stage boot timing, appended to `%LOCALAPPDATA%\FKey\startup.log`, for diagnosing startup delay |
| **Debug Trace** | `core/debug_trace.go` | `FKEY_DEBUG=1`-gated digit-path debug logging with a ring buffer of recently-seen buffer-clear-class keys, for diagnosing IME buffer desync |
| **IME Loop** | `core/ime_loop.go` | Orchestrates hook → engine → injection pipeline. Invalidates smart profile cache on app switch |
| **Text Sender** | `core/text_sender.go` | `SendInput` API text injection with multiple methods |
| **App Detector** | `core/app_detector.go` | Detects foreground process, selects injection profile. Window-aware smart profile cache avoids per-keystroke process tree scans |
| **Coalescer** | `core/coalescer.go` | Batches rapid keystrokes for flicker-free injection |
| **Smart Paste** | `core/smart_paste.go` | Ctrl+Shift+V mojibake detection and fix |
| **Elevation** | `core/elevation.go` | UAC elevation/de-elevation via `ShellExecute` |

### Dedicated Hook Thread

The keyboard and mouse hooks run on their own OS thread (`hook_pump.go`), not on Wails' main thread. A `WH_KEYBOARD_LL`/`WH_MOUSE_LL` hook is only delivered to, and must be pumped by, the thread that installed it — a thread that stops pumping stalls system-wide keyboard input, and Windows silently removes hooks that don't respond within `LowLevelHooksTimeout`. The dedicated thread:

1. Calls `runtime.LockOSThread()` and never unlocks — the thread lives exactly as long as the hooks do, and terminates when the goroutine returns rather than being reused.
2. Installs both hooks, then runs a `GetMessageW`/`TranslateMessage`/`DispatchMessageW` loop to keep them serviced.
3. Shuts down when `PostThreadMessageW` posts `WM_QUIT` to it; on wakeup it unhooks both hooks on that same thread before the goroutine returns.

This thread is started in `main.go` right after IME-loop creation — **before** `application.New()` — so typing works while Wails is still initializing the tray, window, and WebView2. Because the toggle hotkey can now fire from this thread before Wails exists at all, `OnEnabledChanged`'s UI-touching work is nil-guarded on `globalApp` and marshaled onto the Wails main thread via `application.InvokeAsync`; calling it before `application.New()` returns would panic inside Wails.

### Services

| Service | File | Purpose |
|---------|------|---------|
| **Settings** | `services/settings.go` | Windows Registry read/write at `HKCU\SOFTWARE\FKey` |
| **Updater** | `services/updater.go` | GitHub VERSION file check, zip download, batch install |
| **Formatting** | `services/formatting.go` | Unicode text formatting config (bold/italic/underline) |

### Frontend

- `frontend/index.html` — Single-page settings UI
- `frontend/assets/app.js` — Application logic, Wails binding calls
- `frontend/assets/app.css` — Styling
- `bindings.go` — `AppBindings` struct exposes Go methods to JavaScript
- `updater_ui.go` — Update-check background task and the download/install/relaunch dialog flow (package `main`, split out of `main.go` to keep it focused on process startup)

---

## Key Data Flows

### 1. Keystroke Processing

```
User keypress
    │
    ▼
Win32 LowLevelKeyboardProc (keyboard_hook.go)
    │  ├─ defer recover() — panic recovery for resilience under load
    │  ├─ Skip if: injected (FKEY marker), modifier-only, or disabled
    │  ├─ Check hotkey toggle (Ctrl+Shift, etc.)
    │  ├─ Format hotkeys dispatched via goSafe() (non-blocking goroutine)
    │  └─ Detect Shift/CapsLock state
    │
    ▼
ImeLoop.processKey (ime_loop.go)
    │  ├─ Check AppChanged() → clear buffer + invalidate smart profile cache
    │  ├─ Translate VK → macOS keycode (bridge.go)
    │  └─ Call bridge.ProcessKey()
    │
    ▼
Rust FFI: ime_key_ext (lib.rs → engine/mod.rs)
    │  ├─ Check shortcuts (shortcut.rs)
    │  ├─ Buffer management (buffer.rs)
    │  ├─ Vietnamese validation (validation.rs)
    │  ├─ Pattern matching & transform (transform.rs)
    │  └─ Tone/mark placement (syllable.rs + vowel.rs)
    │
    ▼
Result {action, backspace, chars}
    │
    ▼
Text injection (text_sender.go)
    │  ├─ GetSmartAppProfile() — cached per window handle, no per-key process scan
    │  ├─ Coalescer batches if needed
    │  └─ SendInput: backspaces → Unicode chars
    │
    ▼
Text appears in active application
```

### 2. Settings Flow

```
User changes setting in WebView2 UI
    │
    ▼
JavaScript → Wails binding → AppBindings.SaveSettings()
    │
    ▼
services/settings.go → Write to Windows Registry
    │
    ▼
ImeLoop.UpdateSettings() → Rust FFI calls (ime_method, ime_modern, etc.)
```

### 3. Auto-Update Flow

```
App starts → 3s delay → updater.CheckForUpdates()
    │
    ▼
Fetch raw.githubusercontent.com/miken90/fkey/main/VERSION
    │
    ▼
Compare versions (supports pre-release: 1.0.1-pre.368) → If newer:
    │  ├─ Download FKey-vX.X.X-portable.zip
    │  ├─ Create batch script (wait, kill, replace, restart)
    │  └─ Run script, quit app
    │
    ▼
Batch script replaces FKey.exe → Restarts
```

---

## Text Injection Methods

The app detector selects the optimal injection method per application:

| Method | How | Used For |
|--------|-----|----------|
| **Fast** | Separate `SendInput` calls with 5ms delay | Most apps (Notepad, VS Code) |
| **Slow** | Per-character with 5ms key + 20/15ms pre/post delay | Electron apps, browsers |
| **Atomic** | Single `SendInput` call with all inputs | Discord (prevents flicker) |
| **Paste** | Clipboard + `Ctrl+V` | Warp terminal, apps that don't support SendInput |
| **Passthrough** | Skip IME processing, let keys pass through | Remote desktop apps (Parsec) |

### App Profiles

Apps are matched by process name (e.g., `discord.exe`, `code.exe`, `warp.exe`). Each profile specifies:
- Injection method
- Whether to coalesce keystrokes
- Coalescing timer (ms)
- Backspace mode (VK_BACK vs Unicode BS)

---

## FFI Bridge Design

The Go→Rust bridge uses **`syscall.LoadDLL`** (no CGo dependency):

1. `dll_embed.go` embeds `gonhanh_core.dll` as `//go:embed`
2. At startup, DLL is extracted to temp directory
3. `bridge.go` loads DLL via `syscall.LoadDLL(path)`
4. Each FFI function is resolved via `FindProc("ime_*")`
5. Calls use `proc.Call(uintptr(arg1), uintptr(arg2), ...)`
6. Result struct parsed from raw bytes at known offsets

**Keycode translation**: The Rust engine uses macOS keycodes internally (historical). `TranslateToMacKeycode()` in `bridge.go` maps Windows VK codes → macOS keycodes before each FFI call.

---

## Resilience & Performance (v2.3.1)

The keyboard hook callback (`WH_KEYBOARD_LL`) must return quickly — Windows silently removes hooks that exceed `LowLevelHooksTimeout` (~300ms default). Three mechanisms prevent hook removal under system load:

1. **Smart Profile Caching** (`app_detector.go`): `GetSmartAppProfile()` caches the detected profile per window handle. Process tree enumeration (via `CreateToolhelp32Snapshot`) only runs when the foreground window changes, not on every keystroke. Cache is invalidated by `InvalidateSmartProfileCache()` when `AppChanged()` fires in `ime_loop.go`.

2. **Panic Recovery** (`keyboard_hook.go`): `hookCallback` wraps its body in `defer recover()` so that panics from FFI calls, `unsafe.Pointer` operations, or Win32 API calls under memory pressure don't crash the process — the key simply passes through.

3. **goSafe() Helper** (`keyboard_hook.go`): Goroutines spawned from the hook callback (SmartPaste, FormatHotkey handlers) use `goSafe()` which wraps the function in a goroutine with its own `defer recover()`, preventing unrecovered panics from killing the process.

4. **Dedicated Pumping Thread** (`hook_pump.go`): the hooks are installed on and pumped by their own OS thread rather than relying on Wails' main-thread message loop (which historically only started pumping once `app.Run()` was reached) — see "Dedicated Hook Thread" above.

---

## Remote Desktop Compatibility

FKey supports Vietnamese input through remote desktop applications (Parsec, AnyDesk, RDP, etc.) with a two-part strategy:

1. **Remote side — process injected keys** (`keyboard_hook.go`): The hook only skips keys marked with FKey's own `InjectedKeyMarker` (`0x464B4559`). Keys injected by remote desktop hosts (which have `LLKHF_INJECTED` flag but not FKey's marker) are processed normally. This allows FKey on the remote PC to handle keystrokes forwarded by the remote desktop app.

2. **Local side — passthrough mode** (`ime_loop.go`, `app_detector.go`): When a remote desktop client (e.g., Parsec) is the foreground app, FKey skips IME processing entirely (`MethodPassthrough`). Physical keystrokes pass through unmodified so the remote desktop app can forward them. This prevents conflicts when FKey runs on both machines.

---

## Dependencies

### Rust Core
- **Runtime**: None (only `std`)
- **Dev**: `rstest` 0.18 (parameterized tests), `serial_test` 3.0 (sequential test execution)

### Go Platform
- **Framework**: `wails/v3` v3.0.0-alpha.60
- **System**: `golang.org/x/sys` (Windows Registry, process APIs)
- **Text**: `golang.org/x/text` (encoding for mojibake fix)
- **Build**: Go 1.25, PowerShell

### System Requirements
- Windows 10/11 (64-bit)
- WebView2 Runtime (for settings UI)
- No admin rights required (optional elevation)
