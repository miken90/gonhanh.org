package core

import "testing"

// TestIsBufferClearKey covers the H0 fix key class: nav/function/numpad keys
// that IsRelevantKey never routes to processKey, but that must still clear
// the engine buffer before passing through.
func TestIsBufferClearKey(t *testing.T) {
	tests := []struct {
		name     string
		vk       uint16
		expected bool
	}{
		{"left arrow", VK_LEFT, true},
		{"up arrow", VK_UP, true},
		{"right arrow", VK_RIGHT, true},
		{"down arrow", VK_DOWN, true},
		{"page up", VK_PRIOR, true},
		{"page down", VK_NEXT, true},
		{"end", VK_END, true},
		{"home", VK_HOME, true},
		{"insert", VK_INSERT, true},
		{"delete", VK_DELETE, true},
		{"F1", VK_F1, true},
		{"F12", VK_F12, true},
		{"F7 (mid-range)", 0x76, true},
		{"numpad 0", VK_NUMPAD0, true},
		{"numpad 9", 0x69, true},
		{"numpad multiply", 0x6A, true},
		{"numpad divide", VK_DIVIDE, true},

		{"letter A", VK_A, false},
		{"top-row digit 0", VK_0, false},
		{"top-row digit 9", VK_9, false},
		{"space", VK_SPACE, false},
		{"tab", VK_TAB, false},
		{"backspace", VK_BACK, false},
		{"escape", VK_ESCAPE, false},
		{"left win", VK_LWIN, false}, // Win is handled separately (H-Win), not by this key class
		{"right win", VK_RWIN, false},
		{"just below arrows (0x24 Home)", VK_HOME, true},
		{"just above numpad range (0x70 F1)", VK_F1, true},
		{"gap between delete and numpad (0x2F)", 0x2F, false},
		{"gap between numpad and F-keys (0x6F is divide, 0x70 is F1)", 0x6F, true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := IsBufferClearKey(tt.vk)
			if got != tt.expected {
				t.Errorf("IsBufferClearKey(0x%02X) = %v, want %v", tt.vk, got, tt.expected)
			}
		})
	}
}

// TestKeyboardHook_ResetConsumed verifies the consumed-map leak fix: after
// ResetConsumed, no KEYUP suppression counters remain stuck from a prior app.
func TestKeyboardHook_ResetConsumed(t *testing.T) {
	h := NewKeyboardHook()

	h.consumedMu.Lock()
	h.consumed[VK_A] = 2
	h.consumed[VK_0] = 1
	h.consumedMu.Unlock()

	h.ResetConsumed()

	h.consumedMu.Lock()
	defer h.consumedMu.Unlock()
	if len(h.consumed) != 0 {
		t.Errorf("ResetConsumed left %d stale entries, want 0", len(h.consumed))
	}
}
