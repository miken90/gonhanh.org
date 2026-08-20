package core

// IME Loop - orchestrates keyboard hook, Rust engine, and text injection
// This is the main integration point for Vietnamese input processing

import (
	"fmt"
	"sync"
)

// ImeLoop manages the complete IME processing pipeline
type ImeLoop struct {
	hook      *KeyboardHook
	mouseHook *MouseHook
	pump      *hookPump
	bridge    *Bridge
	settings  *ImeSettings
	coalescer *Coalescer
	running   bool
	mu        sync.Mutex

	// Callbacks for UI notification
	OnEnabledChanged func(enabled bool)
}

// ImeSettings holds runtime IME configuration
type ImeSettings struct {
	Enabled            bool
	InputMethod        InputMethod
	ModernTone         bool
	SkipWShortcut      bool
	BracketShortcut    bool
	EscRestore         bool
	FreeTone           bool
	EnglishAutoRestore bool
	AutoCapitalize     bool
}

// DefaultImeSettings returns default settings
func DefaultImeSettings() *ImeSettings {
	return &ImeSettings{
		Enabled:            true,
		InputMethod:        Telex,
		ModernTone:         true,
		SkipWShortcut:      false,
		BracketShortcut:    false,
		EscRestore:         true,
		FreeTone:           false,
		EnglishAutoRestore: false,
		AutoCapitalize:     false, // Default: OFF (user feedback)
	}
}

// NewImeLoop creates a new IME loop
func NewImeLoop() (*ImeLoop, error) {
	bridge, err := GetBridge()
	if err != nil {
		return nil, err
	}

	hook := NewKeyboardHook()
	mouseHook := NewMouseHook()
	settings := DefaultImeSettings()

	loop := &ImeLoop{
		hook:      hook,
		mouseHook: mouseHook,
		pump:      newHookPump(hook, mouseHook),
		bridge:    bridge,
		settings:  settings,
	}

	// Create coalescer with sendFunc callback
	loop.coalescer = NewCoalescer(func(text string, backspaces int, method InjectionMethod) {
		SendTextWithMethod(text, backspaces, method)
	})

	// Set up key processing callback
	hook.OnKeyPressed = loop.processKey
	// H0/H-Win fix: nav/function/numpad keys and Win combos clear the buffer
	// and flush the coalescer without going through processKey.
	hook.OnClearKey = loop.clearAndFlush
	// Mouse click can also move the caret without any key event; same fix.
	mouseHook.OnButtonDown = loop.clearAndFlush

	return loop, nil
}

// clearAndFlush clears the engine buffer and flushes any pending coalesced
// replacement. Used for keys/clicks that invalidate the buffer but don't
// otherwise go through processKey (nav/F-keys/numpad, Win combos, mouse
// clicks).
func (l *ImeLoop) clearAndFlush() {
	l.bridge.Clear()
	l.coalescer.Flush()
}

// Start begins the IME loop
func (l *ImeLoop) Start() error {
	l.mu.Lock()
	defer l.mu.Unlock()

	if l.running {
		return nil
	}

	// Initialize Rust engine
	l.bridge.Initialize()
	l.applySettings()

	// Install the keyboard/mouse hooks on a dedicated, locked OS thread with
	// its own message pump (see hook_pump.go). This must not run on the
	// goroutine that will later call application.New()/app.Run() - it needs
	// to keep pumping independently so typing works while Wails is still
	// initializing, and a LL hook's installing thread must itself be the
	// one pumping or Windows silently removes the hook.
	if err := l.pump.start(); err != nil {
		return err
	}

	l.running = true
	return nil
}

// Stop ends the IME loop
func (l *ImeLoop) Stop() {
	l.mu.Lock()
	defer l.mu.Unlock()

	if !l.running {
		return
	}

	l.pump.stop()
	l.running = false
}

// IsRunning returns whether the IME loop is active
func (l *ImeLoop) IsRunning() bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.running
}

// SetEnabled enables or disables IME processing
func (l *ImeLoop) SetEnabled(enabled bool) {
	l.settings.Enabled = enabled
	l.bridge.SetEnabled(enabled)

	if l.OnEnabledChanged != nil {
		l.OnEnabledChanged(enabled)
	}
}

// Toggle toggles IME enabled state
func (l *ImeLoop) Toggle() bool {
	newState := !l.settings.Enabled
	l.SetEnabled(newState)
	return newState
}

// SetHotkey sets the toggle hotkey
func (l *ImeLoop) SetHotkey(keyCode uint16, ctrl, alt, shift bool) {
	// Detect modifier-only shortcuts (keyCode = 0 means modifiers only)
	modifierOnly := keyCode == 0
	
	l.hook.Hotkey = &KeyboardShortcut{
		KeyCode:      keyCode,
		Ctrl:         ctrl,
		Alt:          alt,
		Shift:        shift,
		ModifierOnly: modifierOnly,
	}
	l.hook.OnHotkey = func() {
		l.Toggle()
	}
}

// UpdateSettings applies new settings to the engine
func (l *ImeLoop) UpdateSettings(settings *ImeSettings) {
	l.settings = settings
	l.applySettings()
}

// applySettings syncs settings to Rust engine
func (l *ImeLoop) applySettings() {
	l.bridge.SetEnabled(l.settings.Enabled)
	l.bridge.SetMethod(l.settings.InputMethod)
	l.bridge.SetModernTone(l.settings.ModernTone)
	l.bridge.SetSkipWShortcut(l.settings.SkipWShortcut)
	l.bridge.SetBracketShortcut(l.settings.BracketShortcut)
	l.bridge.SetEscRestore(l.settings.EscRestore)
	l.bridge.SetFreeTone(l.settings.FreeTone)
	l.bridge.SetEnglishAutoRestore(l.settings.EnglishAutoRestore)
	l.bridge.SetAutoCapitalize(l.settings.AutoCapitalize)
}

// ClearBuffer clears the IME buffer
func (l *ImeLoop) ClearBuffer() {
	l.bridge.Clear()
}

// processKey handles a keystroke through the IME pipeline
// Returns true if the key was handled (should be blocked)
func (l *ImeLoop) processKey(keyCode uint16, shift, capsLock bool) bool {
	// Phase 0 instrumentation: log the digit path (VK 0x30-0x39) when
	// FKEY_DEBUG=1, alongside the ring buffer of untracked keys that
	// preceded it, to confirm/refute hypothesis H0 (stale buffer desync via
	// keys IsRelevantKey never routes here). Zero cost otherwise: the
	// FKeyDebugEnabled bool short-circuits IsNumberKey.
	var dbgStage, dbgText, dbgMethod string
	var dbgBackspaces int
	isDigitDebug := FKeyDebugEnabled && IsNumberKey(keyCode)
	if isDigitDebug {
		defer func() {
			logDigitDebug(fmt.Sprintf(
				"digit vk=0x%02X shift=%v caps=%v stage=%s backspaces=%d text=%q method=%s process=%s ignored_ring=[%s]",
				keyCode, shift, capsLock, dbgStage, dbgBackspaces, dbgText, dbgMethod,
				GetCurrentProcessName(), snapshotIgnoredKeyRing()))
		}()
	}

	if !l.settings.Enabled {
		// IME disabled, flush any pending and pass through
		dbgStage = "disabled"
		l.coalescer.Flush()
		return false
	}

	// Check if foreground app changed - if so, clear buffer and invalidate caches
	if AppChanged() {
		l.bridge.Clear()
		l.coalescer.Flush()
		InvalidateSmartProfileCache()
		// consumed-map leak fix: an elevated destination window (UIPI) can
		// silently eat a KEYUP we're waiting to suppress, leaving a stale
		// counter that would wrongly suppress a future KEYUP in a different,
		// non-elevated app.
		l.hook.ResetConsumed()
	}

	// Check if app requires passthrough (remote desktop apps like Parsec).
	// These apps only forward physical keystrokes, not SendInput-injected events.
	// Skip engine processing entirely so keys pass through to the remote.
	profile := GetSmartAppProfile(GetCurrentProcessName())
	if profile.Method == MethodPassthrough {
		dbgStage = "passthrough"
		return false
	}

	// Translate Windows VK to macOS keycode for Rust engine
	macKeycode := TranslateToMacKeycode(keyCode)
	if macKeycode == 0xFFFF {
		// Key not mapped, flush any pending coalesced text first
		dbgStage = "unmapped"
		l.coalescer.Flush()
		return false
	}

	// Calculate if character should be uppercase
	// For letters: shift XOR capsLock determines uppercase
	// Bug fix: Previously passed capsLock directly, but Rust engine expects
	// the final "is uppercase" state, not the capsLock toggle state
	caps := (shift && !capsLock) || (!shift && capsLock)

	// Process through Rust engine
	result := l.bridge.ProcessKey(macKeycode, caps, false, shift)
	if isDigitDebug {
		dbgMethod = injectionMethodName(profile.Method)
	}

	switch result.Action {
	case ActionNone:
		// No action needed, but flush any pending coalesced text first
		dbgStage = "none"
		l.coalescer.Flush()
		return false

	case ActionSend:
		// Send replacement text
		text := result.GetText()
		backspaces := int(result.Backspace)
		dbgStage = "send"
		dbgText = text
		dbgBackspaces = backspaces

		// Use coalescing if profile says so AND this is a diacritic replacement
		if profile.Coalesce && backspaces > 0 {
			l.coalescer.Queue(text, backspaces, profile.Method, profile.CoalesceMs)
		} else {
			// Send immediately with full profile (includes BackspaceMode)
			l.coalescer.Flush()
			SendTextWithProfile(text, backspaces, profile)
		}
		return true

	case ActionRestore:
		// Restore original text (ESC pressed)
		// Flush pending first, then restore
		l.coalescer.Flush()
		text := result.GetText()
		backspaces := int(result.Backspace)
		dbgStage = "restore"
		dbgText = text
		dbgBackspaces = backspaces
		SendText(text, backspaces)
		return true
	}

	dbgStage = "fallthrough"
	return false
}

// AddShortcut adds a text expansion shortcut
func (l *ImeLoop) AddShortcut(trigger, replacement string) {
	l.bridge.AddShortcut(trigger, replacement)
}

// RemoveShortcut removes a shortcut
func (l *ImeLoop) RemoveShortcut(trigger string) {
	l.bridge.RemoveShortcut(trigger)
}

// ClearShortcuts removes all shortcuts
func (l *ImeLoop) ClearShortcuts() {
	l.bridge.ClearShortcuts()
}
