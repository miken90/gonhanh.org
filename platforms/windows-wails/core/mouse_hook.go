package core

// Low-level mouse hook (Phase 2, H0 companion fix). A mouse click can move
// the caret without ever routing through the keyboard hook, leaving the
// engine's word buffer stale relative to what's on screen - the same class
// of problem as the untracked-key case (H0). This hook only clears the
// buffer and flushes the coalescer on button-down; it does no FFI or
// pointer parsing of its own, mirroring the panic-recovered, non-blocking
// posture of the keyboard hook.

import (
	"log"
	"syscall"
)

const (
	WH_MOUSE_LL    = 14
	WM_LBUTTONDOWN = 0x0201
	WM_RBUTTONDOWN = 0x0204
	WM_MBUTTONDOWN = 0x0207
)

// MouseHook installs a low-level mouse hook that clears the IME buffer on
// left/right/middle button-down.
type MouseHook struct {
	hookID   uintptr
	hookProc uintptr // prevent GC

	// OnButtonDown is called synchronously on any left/right/middle
	// button-down. Must stay cheap and non-blocking, same constraint as
	// KeyboardHook.OnClearKey.
	OnButtonDown func()
}

// NewMouseHook creates a new (unstarted) mouse hook.
func NewMouseHook() *MouseHook {
	return &MouseHook{}
}

// Start begins mouse interception.
func (m *MouseHook) Start() error {
	if m.hookID != 0 {
		return nil // Already started
	}

	m.hookProc = syscall.NewCallback(m.hookCallback)

	hMod, _, _ := procGetModuleHandle.Call(0)

	hookID, _, err := procSetWindowsHookEx.Call(
		WH_MOUSE_LL,
		m.hookProc,
		hMod,
		0,
	)

	if hookID == 0 {
		return err
	}

	m.hookID = hookID
	return nil
}

// Stop ends mouse interception.
func (m *MouseHook) Stop() {
	if m.hookID != 0 {
		procUnhookWindowsHookEx.Call(m.hookID)
		m.hookID = 0
	}
}

// hookCallback is the low-level mouse procedure. Deliberately trivial: it
// only compares wParam and, on a button-down, calls OnButtonDown - no
// unsafe pointer parsing of the MSLLHOOKSTRUCT payload, since position data
// isn't needed to clear the buffer.
func (m *MouseHook) hookCallback(nCode int, wParam uintptr, lParam uintptr) uintptr {
	defer func() {
		if r := recover(); r != nil {
			log.Printf("[MouseHook] recovered from panic: %v", r)
		}
	}()

	if nCode >= 0 {
		switch wParam {
		case WM_LBUTTONDOWN, WM_RBUTTONDOWN, WM_MBUTTONDOWN:
			if m.OnButtonDown != nil {
				m.OnButtonDown()
			}
		}
	}

	ret, _, _ := procCallNextHookEx.Call(m.hookID, uintptr(nCode), wParam, lParam)
	return ret
}
