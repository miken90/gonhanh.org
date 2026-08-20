package core

// Keyboard hook implementation using Windows low-level keyboard hook
// Port of KeyboardHook.cs from .NET implementation

import (
	"log"
	"sync"
	"syscall"
	"time"
	"unsafe"
)

const (
	WH_KEYBOARD_LL = 13
	WM_KEYDOWN     = 0x0100
	WM_KEYUP       = 0x0101
	WM_SYSKEYDOWN  = 0x0104
	WM_SYSKEYUP    = 0x0105
	LLKHF_INJECTED = 0x10
)

// Virtual key codes (Windows)
const (
	VK_BACK    = 0x08
	VK_TAB     = 0x09
	VK_RETURN  = 0x0D
	VK_SHIFT   = 0x10
	VK_CONTROL = 0x11
	VK_MENU    = 0x12 // Alt
	VK_CAPITAL = 0x14 // CapsLock
	VK_ESCAPE  = 0x1B
	VK_SPACE   = 0x20
	VK_A       = 0x41
	VK_Z       = 0x5A
	VK_0       = 0x30
	VK_9       = 0x39

	// OEM keys
	VK_OEM_1      = 0xBA // ;:
	VK_OEM_2      = 0xBF // /?
	VK_OEM_3      = 0xC0 // `~
	VK_OEM_4      = 0xDB // [{
	VK_OEM_5      = 0xDC // \|
	VK_OEM_6      = 0xDD // ]}
	VK_OEM_7      = 0xDE // '"
	VK_OEM_PLUS   = 0xBB // =+
	VK_OEM_COMMA  = 0xBC // ,<
	VK_OEM_MINUS  = 0xBD // -_
	VK_OEM_PERIOD = 0xBE // .>

	// Navigation / function / numpad keys (H0 fix) and Win keys (H-Win fix).
	// None of these pass IsRelevantKey, so without explicit handling they
	// never reach processKey - the engine buffer goes stale relative to
	// what's actually on screen, and a later digit/letter can mutate that
	// stale buffer at the new cursor position.
	VK_PRIOR    = 0x21 // Page Up
	VK_NEXT     = 0x22 // Page Down
	VK_END      = 0x23
	VK_HOME     = 0x24
	VK_LEFT     = 0x25
	VK_UP       = 0x26
	VK_RIGHT    = 0x27
	VK_DOWN     = 0x28
	VK_INSERT   = 0x2D
	VK_DELETE   = 0x2E
	VK_NUMPAD0  = 0x60
	VK_DIVIDE   = 0x6F // last numpad operator key (0x60-0x6F: digits 0-9 + * + - + . + /)
	VK_F1       = 0x70
	VK_F12      = 0x7B
	VK_LWIN     = 0x5B
	VK_RWIN     = 0x5C
)

// KBDLLHOOKSTRUCT matches Windows structure
type KBDLLHOOKSTRUCT struct {
	VkCode      uint32
	ScanCode    uint32
	Flags       uint32
	Time        uint32
	DwExtraInfo uintptr
}

// Win32 API
var (
	user32                  = syscall.NewLazyDLL("user32.dll")
	kernel32                = syscall.NewLazyDLL("kernel32.dll")
	procSetWindowsHookEx    = user32.NewProc("SetWindowsHookExW")
	procUnhookWindowsHookEx = user32.NewProc("UnhookWindowsHookEx")
	procCallNextHookEx      = user32.NewProc("CallNextHookEx")
	procGetModuleHandle     = kernel32.NewProc("GetModuleHandleW")
	procGetKeyState         = user32.NewProc("GetKeyState")
	procGetAsyncKeyState    = user32.NewProc("GetAsyncKeyState")
	procMessageBeep         = user32.NewProc("MessageBeep")
)

// InjectedKeyMarker identifies keys we injected (to skip processing)
// "FKEY" in hex: 0x464B4559
var InjectedKeyMarker = uintptr(0x464B4559)

// slowKeyThreshold/severeKeyThreshold gate diagnostic logging for how long a single
// keystroke's synchronous processing (engine call + injection) took inside the hook.
// This runs on the hook-installing thread and blocks delivery of the NEXT physical
// keystroke until it returns, so a slow keystroke here directly widens the window
// during which fast typing can desync the engine's model from what's on screen.
// Logging is done off a buffered, non-blocking channel so instrumentation itself
// never adds latency to the hook.
const (
	slowKeyThresholdMs   = 20
	severeKeyThresholdMs = 60
)

type keyTimingRecord struct {
	keyCode  uint16
	duration time.Duration
}

var keyTimingCh = make(chan keyTimingRecord, 64)

var keyTimingLoggerOnce sync.Once

func startKeyTimingLogger() {
	keyTimingLoggerOnce.Do(func() {
		go func() {
			for rec := range keyTimingCh {
				level := "slow"
				if rec.duration.Milliseconds() >= severeKeyThresholdMs {
					level = "SEVERE"
				}
				log.Printf("[Hook] %s key processing: key=0x%X took %v (fast typing may desync if this recurs)",
					level, rec.keyCode, rec.duration)
			}
		}()
	})
}

// reportSlowKeyProcessing logs (asynchronously, non-blocking) when a single keystroke's
// synchronous IME processing exceeds slowKeyThresholdMs, to help diagnose which apps or
// code paths (process detection, FFI, injection) are eating into the hook's latency
// budget without adding any synchronous cost to the hook itself.
func reportSlowKeyProcessing(keyCode uint16, d time.Duration) {
	if d.Milliseconds() < slowKeyThresholdMs {
		return
	}
	startKeyTimingLogger()
	select {
	case keyTimingCh <- keyTimingRecord{keyCode: keyCode, duration: d}:
	default:
		// Channel full - drop rather than block the hook.
	}
}

// KeyboardHook manages low-level keyboard interception
type KeyboardHook struct {
	hookID       uintptr
	hookProc     uintptr // prevent GC
	isProcessing bool
	mu           sync.Mutex

	// Modifier-only hotkey state tracking
	// When modifier-only hotkey matches on KEYDOWN, we set this to true
	// and wait for KEYUP to actually trigger (allows Ctrl+Shift+V to work)
	modifierOnlyPending bool
	pendingModifiers    struct{ ctrl, alt, shift bool }

	// Track consumed keys to suppress their KEYUP events
	// This fixes Firefox address bar bug where KEYUP inserts the raw character
	// Counter (not bool): Vietnamese Telex frequently doubles the same key
	// (aa→â, dd→đ, ss/ff/rr/xx/jj for tones), so under fast/auto-repeat typing a
	// second KEYDOWN for the same VK can arrive before the first KEYUP is delivered.
	// A bool would let one KEYUP clear the flag early and let a raw character leak
	// through for the other press; a counter suppresses exactly as many KEYUPs as
	// KEYDOWNs we consumed.
	consumedMu sync.Mutex
	consumed   map[uint16]int

	// Callbacks
	OnKeyPressed func(keyCode uint16, shift, capsLock bool) bool // returns true if handled
	OnHotkey     func()
	// OnClearKey is called (synchronously, must stay cheap) when a key that
	// invalidates the engine's buffer - but that IsRelevantKey never routes
	// to OnKeyPressed - is seen: nav/F-keys/numpad (H0 fix) or a Win combo
	// (H-Win fix). The callback must clear the engine buffer and flush any
	// pending coalesced replacement.
	OnClearKey func()

	// Hotkey configuration
	Hotkey        *KeyboardShortcut
	HotkeyEnabled bool
}

// KeyboardShortcut represents a keyboard shortcut
type KeyboardShortcut struct {
	KeyCode      uint16
	Ctrl         bool
	Alt          bool
	Shift        bool
	ModifierOnly bool // If true, trigger on modifier release (e.g., Ctrl+Shift)
}

// Matches checks if the shortcut matches current key state
func (ks *KeyboardShortcut) Matches(keyCode uint16, ctrl, alt, shift bool) bool {
	// For modifier-only shortcuts (e.g., Ctrl+Shift), we match when:
	// - KeyCode is 0 (or a modifier VK like VK_SHIFT)
	// - The required modifiers are pressed
	if ks.ModifierOnly {
		// Modifier-only: check modifiers match, ignore keyCode
		return ks.Ctrl == ctrl && ks.Alt == alt && ks.Shift == shift
	}

	return ks.KeyCode == keyCode &&
		ks.Ctrl == ctrl &&
		ks.Alt == alt &&
		ks.Shift == shift
}

// NewKeyboardHook creates a new keyboard hook
func NewKeyboardHook() *KeyboardHook {
	return &KeyboardHook{
		HotkeyEnabled: true,
		consumed:      make(map[uint16]int),
	}
}

// Start begins keyboard interception
func (h *KeyboardHook) Start() error {
	if h.hookID != 0 {
		return nil // Already started
	}

	// Create callback
	h.hookProc = syscall.NewCallback(h.hookCallback)

	// Get module handle
	hMod, _, _ := procGetModuleHandle.Call(0)

	// Install hook
	hookID, _, err := procSetWindowsHookEx.Call(
		WH_KEYBOARD_LL,
		h.hookProc,
		hMod,
		0,
	)

	if hookID == 0 {
		return err
	}

	h.hookID = hookID
	return nil
}

// Stop ends keyboard interception
func (h *KeyboardHook) Stop() {
	if h.hookID != 0 {
		procUnhookWindowsHookEx.Call(h.hookID)
		h.hookID = 0
	}
}

// hookCallback is the low-level keyboard procedure
func (h *KeyboardHook) hookCallback(nCode int, wParam uintptr, lParam uintptr) uintptr {
	// Panic recovery — prevent app crash under resource pressure.
	// If FFI, unsafe pointer ops, or Win32 API calls panic, the key
	// just passes through instead of crashing the entire process.
	defer func() {
		if r := recover(); r != nil {
			log.Printf("[Hook] recovered from panic: %v", r)
		}
	}()

	hookStruct := (*KBDLLHOOKSTRUCT)(unsafe.Pointer(lParam))

	// Skip our own injected keys (prevents processing loop)
	if hookStruct.DwExtraInfo == InjectedKeyMarker {
		ret, _, _ := procCallNextHookEx.Call(h.hookID, uintptr(nCode), wParam, lParam)
		return ret
	}

	// NOTE: We intentionally do NOT skip other injected keys (LLKHF_INJECTED).
	// Remote desktop apps (Parsec, AnyDesk, RDP, etc.) forward keystrokes
	// via SendInput which sets LLKHF_INJECTED. Skipping those would prevent
	// FKey from processing Vietnamese input on the remote machine.
	// The InjectedKeyMarker check above is sufficient to prevent self-loops.

	// Only skip physical keys if already processing AND it's an injected event
	// This fixes the bug where physical KEYUP slips through during slow-mode injection
	if h.isProcessing {
		// isProcessing but not injected = physical key during SendInput
		// We should NOT skip these - need to check if KEYUP should be suppressed
		// Fall through to normal handling
	}

	keyCode := uint16(hookStruct.VkCode)

	// Handle KEYUP events
	if nCode >= 0 && (wParam == WM_KEYUP || wParam == WM_SYSKEYUP) {
		// First: Check if this KEYUP should be suppressed because we consumed the KEYDOWN
		// This fixes Firefox address bar bug where KEYUP inserts the raw character
		h.consumedMu.Lock()
		if h.consumed[keyCode] > 0 {
			h.consumed[keyCode]--
			if h.consumed[keyCode] == 0 {
				delete(h.consumed, keyCode)
			}
			h.consumedMu.Unlock()
			return 1 // Block KEYUP for consumed keys
		}
		h.consumedMu.Unlock()

		// Handle modifier-only hotkeys
		isShiftKey := keyCode == VK_SHIFT || keyCode == VK_LSHIFT || keyCode == VK_RSHIFT
		isCtrlKey := keyCode == VK_CONTROL || keyCode == VK_LCONTROL || keyCode == VK_RCONTROL
		isAltKey := keyCode == VK_MENU || keyCode == VK_LMENU || keyCode == VK_RMENU

		// If a modifier key is released and we have a pending modifier-only toggle
		if (isShiftKey || isCtrlKey || isAltKey) && h.modifierOnlyPending {
			h.modifierOnlyPending = false
			if h.OnHotkey != nil {
				h.OnHotkey()
			}
			// Don't consume KEYUP, let it pass through
		}
		ret, _, _ := procCallNextHookEx.Call(h.hookID, uintptr(nCode), wParam, lParam)
		return ret
	}

	// Process key down events
	if nCode >= 0 && (wParam == WM_KEYDOWN || wParam == WM_SYSKEYDOWN) {

		// Phase 0 instrumentation (H0): record this key if it's one of the
		// classes IsRelevantKey never routes to processKey, so a digit-path
		// debug log can show what untracked keys preceded it. No-op unless
		// FKEY_DEBUG=1.
		recordIgnoredKey(keyCode)

		// Get modifier states
		// Note: The key currently being pressed may not be reflected in GetAsyncKeyState yet
		// So we need to account for it based on keyCode
		shift := isKeyDown(VK_SHIFT)
		ctrl := isKeyDown(VK_CONTROL)
		alt := isKeyDown(VK_MENU)
		capsLock := isCapsLockOn()

		// If the current keyCode IS a modifier, ensure it's counted as pressed
		// GetAsyncKeyState may not have updated yet for the key being pressed
		isShiftKey := keyCode == VK_SHIFT || keyCode == VK_LSHIFT || keyCode == VK_RSHIFT
		isCtrlKey := keyCode == VK_CONTROL || keyCode == VK_LCONTROL || keyCode == VK_RCONTROL
		isAltKey := keyCode == VK_MENU || keyCode == VK_LMENU || keyCode == VK_RMENU
		if isShiftKey {
			shift = true
		}
		if isCtrlKey {
			ctrl = true
		}
		if isAltKey {
			alt = true
		}

		// Smart Paste: Ctrl+Shift+V (check BEFORE format hotkeys)
		// Must be checked first to avoid being blocked by format hotkey handlers
		if ctrl && shift && !alt && keyCode == 0x56 { // VK_V
			if IsSmartPasteEnabled() {
				// Mark that a non-modifier key was pressed (prevents modifier-only toggle)
				h.modifierOnlyPending = false
				goSafe(HandleSmartPaste)
				return 1 // Block key
			}
		}

		// Check format hotkeys BEFORE toggle hotkey
		// Need at least Ctrl or Alt modifier for format hotkeys
		if ctrl || alt {
			handler := GetFormatHandler()
			if handler != nil && handler.IsEnabled() {
				// Force fresh detection instead of using cached value
				processName := DetectForegroundApp()
				if processName == "" {
					processName = GetCurrentProcessName()
				}

				// Step 1: Check CUSTOM hotkeys first (app-specific overrides)
				customFormatType := handler.MatchesCustomHotkey(processName, keyCode, ctrl, alt, shift)
				if customFormatType != "" {
					profile := handler.GetProfileForApp(processName)
					log.Printf("[FormatHotkey] CUSTOM key=0x%X formatType=%s process=%s profile=%s",
						keyCode, customFormatType, processName, profile)
					if profile != "disabled" {
						goSafe(func() { handler.HandleFormatHotkey(customFormatType, profile) })
						return 1 // Block key
					}
				}

				// Step 2: Check GLOBAL custom hotkeys (user-defined replacements for defaults)
				globalFormatType := handler.MatchesGlobalHotkey(keyCode, ctrl, alt, shift)
				if globalFormatType != "" {
					// Check if this hotkey is excluded for this app
					if handler.IsHotkeyExcluded(processName, globalFormatType) {
						log.Printf("[FormatHotkey] EXCLUDED key=0x%X formatType=%s process=%s", keyCode, globalFormatType, processName)
						ret, _, _ := procCallNextHookEx.Call(h.hookID, uintptr(nCode), wParam, lParam)
						return ret
					}

					profile := handler.GetProfileForApp(processName)
					log.Printf("[FormatHotkey] GLOBAL key=0x%X formatType=%s process=%s profile=%s",
						keyCode, globalFormatType, processName, profile)
					if profile != "disabled" {
						goSafe(func() { handler.HandleFormatHotkey(globalFormatType, profile) })
						return 1 // Block key
					}
				}

				// Step 3: Check DEFAULT hotkeys (Ctrl+B, Ctrl+I, Ctrl+Alt+S, etc.)
				if ctrl {
					if formatType, matched := IsFormatHotkey(keyCode, ctrl, alt, shift); matched {
						// Check if this formatType has a global custom hotkey override
						globalHotkey := handler.Service().GetGlobalHotkey(formatType)
						if globalHotkey != "" {
							// This formatType uses a custom global hotkey, skip default
							ret, _, _ := procCallNextHookEx.Call(h.hookID, uintptr(nCode), wParam, lParam)
							return ret
						}

						// Check if default hotkey has been overridden by a per-app custom one
						customHotkey := handler.GetCustomHotkey(processName, formatType)
						if customHotkey != "" {
							// This formatType has a custom hotkey, skip default handling
							// Let the key pass through
							ret, _, _ := procCallNextHookEx.Call(h.hookID, uintptr(nCode), wParam, lParam)
							return ret
						}

						// Check if this hotkey is excluded for this app
						if handler.IsHotkeyExcluded(processName, formatType) {
							log.Printf("[FormatHotkey] EXCLUDED key=0x%X formatType=%s process=%s", keyCode, formatType, processName)
							// Don't return 1, let the key pass through to native app
							ret, _, _ := procCallNextHookEx.Call(h.hookID, uintptr(nCode), wParam, lParam)
							return ret
						}

						profile := handler.GetProfileForApp(processName)
						log.Printf("[FormatHotkey] key=0x%X shift=%v formatType=%s process=%s profile=%s",
							keyCode, shift, formatType, processName, profile)
						if profile != "disabled" {
							goSafe(func() { handler.HandleFormatHotkey(formatType, profile) })
							return 1 // Block key
						}
					}
				}
			}
		}

		// Check for toggle hotkey
		// For modifier-only shortcuts (like Ctrl+Shift), trigger when the last modifier is pressed
		if h.HotkeyEnabled && h.Hotkey != nil {
			if h.Hotkey.ModifierOnly {
				// Modifier-only: DON'T trigger immediately on KEYDOWN
				// Instead, set pending flag and trigger on KEYUP
				// This allows Ctrl+Shift+V (Smart Paste) to work without triggering toggle first
				if isShiftKey || isCtrlKey || isAltKey {
					if h.Hotkey.Matches(keyCode, ctrl, alt, shift) {
						// Set pending flag - will trigger on KEYUP if no other key pressed
						h.modifierOnlyPending = true
						h.pendingModifiers.ctrl = ctrl
						h.pendingModifiers.alt = alt
						h.pendingModifiers.shift = shift
						// Don't consume the key, let modifier pass through
					}
				}
			} else if h.Hotkey.Matches(keyCode, ctrl, alt, shift) {
				if h.OnHotkey != nil {
					h.OnHotkey()
				}
				return 1 // Consume the key
			}
		}

		// H-Win fix: Win+key (taskbar shortcuts like Win+1..9) must never enter
		// the engine. Win isn't tracked by GetAsyncKeyState the way Ctrl/Alt/
		// Shift are above, so check it explicitly via isKeyDown.
		win := isKeyDown(VK_LWIN) || isKeyDown(VK_RWIN)

		// H0 fix: nav/function/numpad keys never reach OnKeyPressed (see
		// IsRelevantKey), so without this they'd leave the engine buffer
		// stale. Clear + flush, then pass through. Placed after hotkey
		// matching above so a user-configured hotkey (e.g. Ctrl+F5) still
		// takes priority over this fallback.
		if win || IsBufferClearKey(keyCode) {
			if h.OnClearKey != nil {
				h.OnClearKey()
			}
			ret, _, _ := procCallNextHookEx.Call(h.hookID, uintptr(nCode), wParam, lParam)
			return ret
		}

		// Only process relevant keys for Vietnamese input
		if IsRelevantKey(keyCode) {
			// Skip if Ctrl or Alt is pressed (shortcuts)
			if ctrl || alt {
				// Clear buffer on Ctrl+key or Alt+key combinations
				bridge, _ := GetBridge()
				if bridge != nil {
					bridge.Clear()
				}
				ret, _, _ := procCallNextHookEx.Call(h.hookID, uintptr(nCode), wParam, lParam)
				return ret
			}

			// Handle buffer-clearing keys (TAB only - Space/Enter go through IME)
			if keyCode == VK_TAB {
				bridge, _ := GetBridge()
				if bridge != nil {
					bridge.Clear()
				}
				ret, _, _ := procCallNextHookEx.Call(h.hookID, uintptr(nCode), wParam, lParam)
				return ret
			}

			// Process the key through IME callback
			if h.OnKeyPressed != nil {
				h.mu.Lock()
				h.isProcessing = true
				start := time.Now()
				handled := h.OnKeyPressed(keyCode, shift, capsLock)
				reportSlowKeyProcessing(keyCode, time.Since(start))
				h.isProcessing = false
				h.mu.Unlock()

				if handled {
					// Track this key as consumed so we suppress its KEYUP too
					// This fixes Firefox address bar bug where KEYUP inserts raw char
					h.consumedMu.Lock()
					h.consumed[keyCode]++
					h.consumedMu.Unlock()
					return 1 // Block the original key
				}
			}
		}
	}

	ret, _, _ := procCallNextHookEx.Call(h.hookID, uintptr(nCode), wParam, lParam)
	return ret
}

// ResetConsumed clears the KEYUP-suppression counters. Call this when the
// foreground app changes: an elevated destination window (UIPI) can silently
// eat a KEYUP we're waiting to suppress, leaving a stale counter that would
// wrongly suppress a future KEYUP for the same key after focus moves to a
// non-elevated window.
func (h *KeyboardHook) ResetConsumed() {
	h.consumedMu.Lock()
	for k := range h.consumed {
		delete(h.consumed, k)
	}
	h.consumedMu.Unlock()
}

// isKeyDown checks if a key is currently pressed
func isKeyDown(vk int) bool {
	ret, _, _ := procGetAsyncKeyState.Call(uintptr(vk))
	return (ret & 0x8000) != 0
}

// isCapsLockOn checks if CapsLock is toggled on
func isCapsLockOn() bool {
	ret, _, _ := procGetKeyState.Call(uintptr(VK_CAPITAL))
	return (ret & 0x0001) != 0
}

// IsLetterKey checks if virtual key is a letter (A-Z)
func IsLetterKey(vk uint16) bool {
	return vk >= VK_A && vk <= VK_Z
}

// IsNumberKey checks if virtual key is a number (0-9)
func IsNumberKey(vk uint16) bool {
	return vk >= VK_0 && vk <= VK_9
}

// IsBufferClearKey reports whether vk is a navigation, function, or numpad
// key that IsRelevantKey never routes to processKey, but that moves the
// cursor, changes focus, or otherwise invalidates the engine's in-memory
// word buffer (hypothesis H0). These must clear the buffer and flush any
// pending coalesced replacement before passing through, or a later
// digit/letter keystroke can mutate stale "ghost" text at the new cursor
// position.
func IsBufferClearKey(vk uint16) bool {
	switch {
	case vk >= VK_LEFT && vk <= VK_DOWN: // arrows
		return true
	case vk >= VK_PRIOR && vk <= VK_HOME: // PgUp/PgDn/End/Home
		return true
	case vk == VK_INSERT || vk == VK_DELETE:
		return true
	case vk >= VK_F1 && vk <= VK_F12:
		return true
	case vk >= VK_NUMPAD0 && vk <= VK_DIVIDE: // numpad digits + operators
		return true
	}
	return false
}

// IsRelevantKey checks if key should be processed by IME
func IsRelevantKey(vk uint16) bool {
	// Letters
	if IsLetterKey(vk) {
		return true
	}
	// Numbers
	if IsNumberKey(vk) {
		return true
	}
	// Special keys
	switch vk {
	case VK_BACK, VK_SPACE, VK_RETURN, VK_TAB, VK_ESCAPE,
		VK_OEM_4, VK_OEM_6, VK_OEM_PERIOD, VK_OEM_COMMA, VK_OEM_2,
		VK_OEM_1, VK_OEM_7, VK_OEM_MINUS, VK_OEM_PLUS:
		return true
	}
	return false
}

// MessageBeep sound types
const (
	MB_OK              = 0x00000000 // Default beep
	MB_ICONHAND        = 0x00000010 // Critical stop
	MB_ICONQUESTION    = 0x00000020 // Question
	MB_ICONEXCLAMATION = 0x00000030 // Exclamation
	MB_ICONASTERISK    = 0x00000040 // Asterisk (info)
)

// PlayBeep plays a Windows system beep sound
// soundType: true = Vietnamese on (higher pitch), false = English (lower pitch)
func PlayBeep(isVietnamese bool) {
	if isVietnamese {
		// Higher pitch beep for Vietnamese
		procMessageBeep.Call(uintptr(MB_ICONASTERISK))
	} else {
		// Lower pitch beep for English
		procMessageBeep.Call(uintptr(MB_OK))
	}
}

// goSafe runs fn in a goroutine with panic recovery.
// Use for goroutines spawned from the hook callback to prevent
// unrecovered panics from crashing the process.
func goSafe(fn func()) {
	go func() {
		defer func() {
			if r := recover(); r != nil {
				log.Printf("[Hook] goroutine panic: %v", r)
			}
		}()
		fn()
	}()
}
