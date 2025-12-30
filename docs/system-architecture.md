# FKey: System Architecture

> **Note**: FKey v1.6.0 - Windows-only Vietnamese keyboard input

## High-Level Architecture

```
┌──────────────────────────────────────────┐
│      Windows Application (WPF/.NET 8)    │
│                                          │
│  ┌────────────────────────────────┐     │
│  │   WPF System Tray UI            │     │
│  │  • Input method selector        │     │
│  │  • Enable/disable toggle        │     │
│  │  • Settings, About, Update      │     │
│  └────────────┬────────────────────┘     │
│               │                          │
│  ┌────────────▼────────────────────┐     │
│  │ SetWindowsHookEx Keyboard Hook  │     │
│  │ • Intercepts WH_KEYBOARD_LL     │     │
│  │ • SendInput for text            │     │
│  │ • Global hotkey detection       │     │
│  └────────────┬────────────────────┘     │
│               │                          │
│  ┌────────────▼────────────────────┐     │
│  │   RustBridge.cs (P/Invoke)      │     │
│  │  • P/Invoke DLL function calls  │     │
│  │  • UTF-32 interop               │     │
│  │  • 11 FFI methods               │     │
│  └────────────┬────────────────────┘     │
└───────────────┼──────────────────────────┘
                │
           P/Invoke
                ↓
         ┌─────────────────────────────────────────────┐
         │     Rust Core Engine (Platform-Agnostic)   │
         │     7-Stage Validation-First Pipeline       │
         └─────────────────────────────────────────────┘
                            ↓
         ┌─────────────────────────────────────────────┐
         │          Input Method Layer                 │
         │  ┌──────────┐  ┌──────────┐  ┌──────────┐  │
         │  │ Telex    │  │ VNI      │  │ Shortcut │  │
         │  │ s/f/r/x/j   1-5/6-8/9  │  │ Priority │  │
         │  └──────────┘  └──────────┘  └──────────┘  │
         └─────────────────────────────────────────────┘
                            ↓
         ┌─────────────────────────────────────────────┐
         │     7-Stage Processing Pipeline             │
         │  1. Stroke (đ/Đ)                           │
         │  2. Tone Marks (sắc/huyền/hỏi/ngã/nặng)    │
         │  3. Vowel Marks (circumflex/horn/breve)    │
         │  4. Mark Removal (revert)                  │
         │  5. W-Vowel (Telex "w"→"ư")               │
         │  6. Normal Letter                          │
         │  7. Shortcut Expansion                     │
         └─────────────────────────────────────────────┘
                            ↓
         ┌─────────────────────────────────────────────┐
         │  Validation Rules (Before Transform)        │
         │  1. Must have vowel                         │
         │  2. Valid initials only                     │
         │  3. All chars parsed                        │
         │  4. Spelling rules (c/k/g restrictions)     │
         │  5. Valid finals only                       │
         └─────────────────────────────────────────────┘
                            ↓
         ┌─────────────────────────────────────────────┐
         │  Transform & Data Layer                     │
         │  • Vowel table: 72 entries                  │
         │  • Character mappings                       │
         │  • Phonology constants                      │
         └─────────────────────────────────────────────┘
                            ↓
         ┌─────────────────────────────────────────────┐
         │  Result Structure                           │
         │  • action: None/Send/Restore                │
         │  • backspace: N chars to delete             │
         │  • chars: [u32; 32] UTF-32 output          │
         │  • count: valid char count                  │
         └─────────────────────────────────────────────┘
```

## Data Flow: Keystroke to Output

### Example: Typing "á" in Telex

```
User types: [a] then [s]

Step 1: Key 'a' pressed
  ├─ KeyboardHook captures WM_KEYDOWN
  ├─ RustBridge.ProcessKey(vkCode=0x41, caps=false, shift=false)
  ├─ Rust: ime_key() called
  ├─ Engine:
  │  ├─ Append 'a' to buffer
  │  ├─ Validate: "a" is valid (vowel alone)
  │  ├─ No transform yet (single char, waiting for next)
  │  └─ Return Action::None (pass through)
  ├─ C#: No action, let 'a' appear naturally
  └─ Output: User sees 'a' typed

Step 2: Key 's' pressed (sắc mark in Telex)
  ├─ KeyboardHook captures WM_KEYDOWN
  ├─ RustBridge.ProcessKey(vkCode=0x53, caps=false, shift=false)
  ├─ Rust: ime_key() called
  ├─ Engine:
  │  ├─ Check buffer context: "a" + "s" → sắc mark
  │  ├─ Validation: "á" is valid Vietnamese vowel
  │  ├─ Transform: Apply sắc mark to 'a' → 'á'
  │  ├─ Check shortcuts: No expansion needed
  │  ├─ Return Action::Send {
  │  │    backspace: 1,  // Delete 'a'
  │  │    chars: ['á']   // Insert 'á'
  │  └─ }
  ├─ C#:
  │  ├─ TextSender.SendText("á", 1)
  │  ├─ Send 1 backspace (delete 'a')
  │  ├─ Send 'á' (via SendInput + KEYEVENTF_UNICODE)
  │  └─ 's' keystroke consumed (not passed through)
  └─ Output: User sees 'á' (exactly 1 character)

Result: "á" displayed instead of "as"
Latency: ~0.2-0.5ms total (Rust engine: <0.1ms)
```

### Example: Typing "không" with Shortcut

```
User types: [k] [h] [o] [n] [g] [space]

Setup: User defined shortcut "khong" → "không"

Processing:
  Step 1-4: Build buffer "khon" → valid syllable, wait
  Step 5: Shortcut lookup
    ├─ Check if "khong" matches any user abbreviation
    ├─ Match found: "khong" → "không"
    └─ Return: backspace: 5, chars: ['k','h','ô','n','g']

  TextSender execution:
    ├─ Delete 5 chars (k, h, o, n, g)
    ├─ Insert 5 chars (k, h, ô, n, g)
    └─ No change visible but ô is now correct diacritic
```

## FFI Interface Specification

### Function Signatures (C ABI)

```c
// Initialize engine (call once)
void ime_init(void);

// Process keystroke
typedef struct {
    uint32_t chars[32];      // UTF-32 output characters
    uint8_t action;          // 0=None, 1=Send, 2=Restore
    uint8_t backspace;       // Number of chars to delete
    uint8_t count;           // Number of valid chars
    uint8_t _pad;            // Padding for alignment
} ImeResult;

ImeResult* ime_key(uint16_t keycode, bool caps, bool ctrl);

// Set input method (0=Telex, 1=VNI)
void ime_method(uint8_t method);

// Enable/disable engine
void ime_enabled(bool enabled);

// Clear buffer (word boundary)
void ime_clear(void);

// Free result (caller must call this exactly once per ime_key)
void ime_free(ImeResult* result);
```

### Action Types

| Value | Name | Meaning | Response |
|-------|------|---------|----------|
| 0 | None | No transformation, pass key through | Send key to app |
| 1 | Send | Transform matched, replace text | Backspace + insert |
| 2 | Restore | Undo previous transform | Not currently used |

### Memory Ownership

- **FFI Responsibility**: Rust engine allocates Result struct
- **Caller Responsibility**: C# must call `ime_free(result)` to deallocate
- **Safety**: Use try/finally to guarantee cleanup even on exceptions

## Windows Platform Integration

### SetWindowsHookEx Keyboard Hook

```csharp
// System-wide low-level keyboard hook
_hookId = SetWindowsHookEx(
    WH_KEYBOARD_LL,           // Low-level keyboard hook
    HookCallback,             // Our handler
    IntPtr.Zero,              // Use current module
    0                         // All threads (system-wide)
);
```

### Text Injection via SendInput

**Unicode injection (KEYEVENTF_UNICODE)**:
```csharp
// Batched Unicode injection - all characters in single SendInput call
var inputs = new INPUT[text.Length * 2]; // 2 events per char (down + up)
foreach (char c in text) {
    inputs[idx++] = new INPUT { // Key down
        wVk = 0,
        wScan = c,              // UTF-16 character code
        dwFlags = KEYEVENTF_UNICODE,
        dwExtraInfo = marker    // Mark as injected to avoid re-processing
    };
    inputs[idx++] = new INPUT { // Key up
        wVk = 0, wScan = c,
        dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
        dwExtraInfo = marker
    };
}
SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<INPUT>());
```

### Text Replacement Strategy

| Mode | Delay | Apps | Method |
|------|-------|------|--------|
| **Fast** | 10ms after backspace | Notepad, Word, standard apps | Batch SendInput |
| **Slow** | 15ms + 20ms + 5ms/char | Electron, terminals, browsers | Character-by-character |

### App Detection (AppDetector.cs)

**Slow Apps** (require delays for compatibility):
- Electron apps: Claude, Notion, Slack, Discord, VS Code, Cursor
- Terminals: Windows Terminal, cmd, PowerShell
- Browsers: Chrome, Edge, Firefox
- IDEs: VS Code, Cursor, Obsidian, Figma

**Fast Apps** (default):
- Notepad, Word, Excel, native Windows apps

### Keyboard Event Processing Architecture

**Current State**: Async queue architecture COMPLETE (Phase 3 - hook wired to queue)

**Components**:
1. **KeyEvent struct** (`Core/KeyEventQueue.cs`)
   - Readonly struct (stack-allocated, lightweight)
   - Fields: VirtualKeyCode, Shift, CapsLock, Timestamp
   - Captures all data needed for processing

2. **KeyEventQueue class** (`Core/KeyEventQueue.cs`)
   - Thread-safe queue using ConcurrentQueue (lock-free)
   - AutoResetEvent for efficient signaling
   - Producer: Hook callback (enqueues in <1μs, non-blocking)
   - Consumer: Worker thread (dequeues and processes)
   - Atomic disposal with Interlocked for thread-safe cleanup

3. **KeyboardWorker class** (`Core/KeyboardWorker.cs`)
   - Dedicated background thread processor
   - AboveNormal priority for responsiveness
   - ProcessLoop with graceful shutdown (timeout-based)
   - Error handling via catch-all with Debug.WriteLine
   - OnKeyProcess callback delegates to App.ProcessKeyFromWorker

4. **KeyboardHook integration** (`Core/KeyboardHook.cs` + `App.xaml.cs`)
   - Hook.SetQueue(queue) wires hook to async queue
   - HookCallback enqueues events in ASYNC MODE (lines 209-213)
   - Legacy OnKeyPressed marked @Obsolete, kept for reference only
   - Flow: Hook → Queue → Worker → ProcessKeyFromWorker → TextSender

**Migration complete**: Legacy synchronous path deprecated, async flow active

### Engine Result Cases

| Case | Action | Backspace | Output | Example (Telex) | Example (VNI) |
|------|--------|-----------|--------|-----------------|---------------|
| **Pass-through** | None | 0 | - | Normal letters, ctrl+key | Normal letters, ctrl+key |
| **Mark (dấu thanh)** | Send | 1 | vowel+mark | `as` → `á` | `a1` → `á` |
| **Tone (dấu mũ/móc)** | Send | 1+ | vowel+tone | `aa` → `â`, `ow` → `ơ` | `a6` → `â`, `o7` → `ơ` |
| **Stroke (đ)** | Send | 1+ | đ | `dd` → `đ` | `d9` → `đ` |
| **Compound ươ** | Send | 2 | ươ | `uow` → `ươ` | `u7o7` → `ươ` |
| **Mark reposition** | Send | 2+ | repositioned | `hoaf` → `hoà` | `hoa2` → `hoà` |
| **Revert (double key)** | Send | 1+ | original+key | `ass` → `as` | `a11` → `a1` |
| **Word shortcut** | Send | N | expanded | `vn ` → `Việt Nam ` | same |
| **W as ư (Telex)** | Send | 0 | ư | `w` → `ư`, `nhw` → `như` | N/A |

### Global Hotkey Toggle

```csharp
// Check hotkey match in KeyboardHook.cs
if (HotkeyEnabled && Hotkey != null && Hotkey.Matches(keyCode, ctrl, alt, shift))
{
    OnHotkeyTriggered?.Invoke();  // Toggle Vietnamese/English
    return (IntPtr)1;              // Consume event
}
```

**Default**: Ctrl+Space (configurable via AdvancedSettingsWindow)

## Component Interactions

### Initialization Sequence (Windows)
```
1. App.OnStartup()
   ├─ EnsureSingleInstance() (Mutex)
   ├─ RustBridge.Initialize() → ime_init()
   ├─ SettingsService.Load() (Registry)
   ├─ ShortcutsManager.Load() (Registry)
   ├─ ApplySettings() → sync to Rust engine
   ├─ Create async pipeline:
   │  ├─ KeyEventQueue (thread-safe ConcurrentQueue)
   │  ├─ KeyboardWorker (background thread)
   │  └─ Set OnKeyProcess callback
   ├─ KeyboardHook.Start() + SetQueue(queue) → async mode
   │  └─ Start worker BEFORE hook (ensure events processed)
   ├─ TrayIcon.Initialize() → NotifyIcon
   └─ Show OnboardingWindow (if first run)
```

### Runtime Flow (Async Queue Architecture)
```
User types key
   ↓
KeyboardHook.HookCallback (WH_KEYBOARD_LL) - <50μs
   ↓
Extract vkCode + modifier state
   ↓
Check global hotkey (Ctrl+Space) → Toggle Vietnamese
   ↓
Check if async mode (_queue != null):
   ├─ YES (ASYNC MODE - Phase 3):
   │  ├─ Enqueue KeyEvent to queue (<1μs)
   │  ├─ Return (IntPtr)1 immediately - block original key
   │  └─ Hook callback exits (<1ms total)
   │       ↓
   │  [Worker Thread - runs concurrently]
   │  KeyboardWorker.ProcessLoop
   │       ↓
   │  Dequeue event (blocks on AutoResetEvent)
   │       ↓
   │  ProcessKeyFromWorker(evt)
   │       ├─ RustBridge.ProcessKey()
   │       │  ├─ Translate Windows VK → macOS keycode
   │       │  ├─ Call ime_key(keycode, caps, shift)
   │       │  ├─ Receive ImeResult
   │       │  └─ Return (backspaceCount, chars) tuple
   │       ↓
   │  If transformation:
   │       ├─ TextSender.SendText(text, backspaces)
   │       │  ├─ SendBackspaces (batch SendInput)
   │       │  ├─ Thread.Sleep(10-15ms) - DOESN'T block hook
   │       │  └─ SendUnicodeText (batch SendInput)
   │       └─ Output visible to user
   │
   └─ NO (LEGACY MODE - deprecated):
      ├─ Call OnKeyPressed event (synchronous)
      └─ Process in hook callback thread (BLOCKS hook)
```

## Advanced Features

### 5 Advanced Settings

1. **Skip W Shortcut** (`ime_skip_w_shortcut`)
   - Controls w→ư shortcut in Telex mode
   - Default: false (w→ư shortcut active)

2. **ESC Restore** (`ime_esc_restore`)
   - ESC key restores raw ASCII input
   - Default: true

3. **Free Tone** (`ime_free_tone`)
   - Enable free tone placement without validation
   - Default: false (validation active)

4. **English Auto-Restore** (`ime_english_auto_restore`)
   - Auto-detect and restore English words (text, expect, user, etc.)
   - Default: false

5. **Auto-Capitalize** (`ime_auto_capitalize`)
   - Auto-capitalize after sentence-ending punctuation (`.` `!` `?` Enter)
   - Space is neutral key (does not reset pending_capitalize)
   - Default: true

### Global Hotkey Toggle

- Default: Ctrl+Space (configurable via AdvancedSettingsWindow)
- KeyboardShortcut model: KeyCode + Modifiers (Ctrl/Alt/Shift/Win)
- HotkeyRecorder UserControl for recording shortcuts with keycap-style UI
- System shortcut conflict detection (blocks Ctrl+C/V/X/A/Z/Y, Alt+Tab/F4)
- Registry persistence: `HKCU\SOFTWARE\GoNhanh\ToggleHotkey`

### Shortcuts Manager

- User-defined abbreviations (vn→Việt Nam, ko→không)
- Registry persistence at `HKCU\SOFTWARE\GoNhanh\Shortcuts`
- Auto-sync with Rust engine via FFI
- Default Vietnamese abbreviations: vn, hn, hcm, ko, dc, vs, ms

### Registry Settings

- Path: `HKCU\SOFTWARE\GoNhanh`
- 5 advanced settings stored as DWord values
- ToggleHotkey stored as string value (KeyCode:Modifiers format)
- Shortcuts stored in subkey with trigger→replacement mapping
- Auto-start: `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run`

## Performance Characteristics

### Latency Budget
| Component | Time | Notes |
|-----------|------|-------|
| KeyboardHook callback | ~50μs | System kernel time |
| Rust ime_key() | ~100-200μs | Engine processing |
| RustBridge P/Invoke | ~50μs | FFI overhead |
| SendInput | ~100-200μs | Text injection |
| **Total** | **~300-500μs** | <1ms requirement met |

### Memory Profile
| Component | Size | Notes |
|-----------|------|-------|
| Rust DLL | ~150KB | Tables + code |
| WPF runtime | ~10MB | Standard .NET overhead |
| Buffer (64 chars) | ~200B | Circular buffer |
| **Total** | **~10-15MB** | Meets requirement |

### Scalability
- **Single instance**: Mutex-protected (one app per user)
- **Thread-safe**: ENGINE global protected by Mutex
- **Continuous**: No memory leaks (tested with 600+ tests)
- **No limits**: Type indefinitely without performance degradation

---

**Last Updated**: 2025-12-30
**Architecture Version**: 2.1 (Windows-only)
**Platform**: Windows 10/11 (.NET 8, WPF)
**Diagram Format**: ASCII (compatible with all documentation viewers)

**Resolved Issues**:
- ✅ Race condition with fast typing (FIXED via async queue architecture Phase 3)

**Complete Features**:
- ✅ 11 RustBridge FFI methods
- ✅ 5 advanced settings with Registry persistence
- ✅ ShortcutsManager service
- ✅ Global hotkey toggle (configurable)
- ✅ Auto-start configuration
- ✅ Unicode text injection (clipboard-safe)
- ✅ Full Vietnamese input support (Telex/VNI)
- ✅ Async queue keyboard processing (Phase 3 - race condition fixed)
