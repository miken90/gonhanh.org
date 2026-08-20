package core

// Format-hotkey routing (custom per-app > global custom > default),
// extracted out of keyboard_hook.go's hookCallback so that function stays
// readable. Identical behavior and log lines to the inline version.

import "log"

// formatHotkeyAction is the outcome of routeFormatHotkey: what hookCallback
// should do with the current KEYDOWN event.
type formatHotkeyAction int

const (
	// formatHotkeyNone: no format hotkey matched (or none applicable) -
	// continue normal key processing (toggle hotkey, IME, etc.)
	formatHotkeyNone formatHotkeyAction = iota
	// formatHotkeyBlock: a format hotkey handled the key - block it (return 1).
	formatHotkeyBlock
	// formatHotkeyPassThrough: routing decided the key must pass through to
	// the native app unmodified (call CallNextHookEx).
	formatHotkeyPassThrough
)

// routeFormatHotkey implements the 3-tier format-hotkey routing: custom
// per-app hotkeys first, then global custom hotkeys, then default hotkeys
// (Ctrl+B, Ctrl+I, Ctrl+Alt+S, etc.).
func routeFormatHotkey(keyCode uint16, ctrl, alt, shift bool) formatHotkeyAction {
	// Need at least Ctrl or Alt modifier for format hotkeys
	if !ctrl && !alt {
		return formatHotkeyNone
	}

	handler := GetFormatHandler()
	if handler == nil || !handler.IsEnabled() {
		return formatHotkeyNone
	}

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
			return formatHotkeyBlock
		}
	}

	// Step 2: Check GLOBAL custom hotkeys (user-defined replacements for defaults)
	globalFormatType := handler.MatchesGlobalHotkey(keyCode, ctrl, alt, shift)
	if globalFormatType != "" {
		// Check if this hotkey is excluded for this app
		if handler.IsHotkeyExcluded(processName, globalFormatType) {
			log.Printf("[FormatHotkey] EXCLUDED key=0x%X formatType=%s process=%s", keyCode, globalFormatType, processName)
			return formatHotkeyPassThrough
		}

		profile := handler.GetProfileForApp(processName)
		log.Printf("[FormatHotkey] GLOBAL key=0x%X formatType=%s process=%s profile=%s",
			keyCode, globalFormatType, processName, profile)
		if profile != "disabled" {
			goSafe(func() { handler.HandleFormatHotkey(globalFormatType, profile) })
			return formatHotkeyBlock
		}
	}

	// Step 3: Check DEFAULT hotkeys (Ctrl+B, Ctrl+I, Ctrl+Alt+S, etc.)
	if ctrl {
		if formatType, matched := IsFormatHotkey(keyCode, ctrl, alt, shift); matched {
			// Check if this formatType has a global custom hotkey override
			globalHotkey := handler.Service().GetGlobalHotkey(formatType)
			if globalHotkey != "" {
				// This formatType uses a custom global hotkey, skip default
				return formatHotkeyPassThrough
			}

			// Check if default hotkey has been overridden by a per-app custom one
			customHotkey := handler.GetCustomHotkey(processName, formatType)
			if customHotkey != "" {
				// This formatType has a custom hotkey, skip default handling
				// Let the key pass through
				return formatHotkeyPassThrough
			}

			// Check if this hotkey is excluded for this app
			if handler.IsHotkeyExcluded(processName, formatType) {
				log.Printf("[FormatHotkey] EXCLUDED key=0x%X formatType=%s process=%s", keyCode, formatType, processName)
				// Don't block, let the key pass through to native app
				return formatHotkeyPassThrough
			}

			profile := handler.GetProfileForApp(processName)
			log.Printf("[FormatHotkey] key=0x%X shift=%v formatType=%s process=%s profile=%s",
				keyCode, shift, formatType, processName, profile)
			if profile != "disabled" {
				goSafe(func() { handler.HandleFormatHotkey(formatType, profile) })
				return formatHotkeyBlock
			}
		}
	}

	return formatHotkeyNone
}
