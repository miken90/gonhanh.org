package services

// Settings service using Windows Registry
// Port of SettingsService.cs from .NET implementation
// Compatible with existing .NET settings (reads from GoNhanh key for migration)

import (
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"

	"golang.org/x/sys/windows"
	"golang.org/x/sys/windows/registry"
)

const (
	RegistryKeyPath      = `SOFTWARE\FKey`
	LegacyRegistryPath   = `SOFTWARE\GoNhanh` // For migration
	AutoStartKeyPath     = `SOFTWARE\Microsoft\Windows\CurrentVersion\Run`
	ShortcutsKeyPath     = `SOFTWARE\FKey\Shortcuts`
	LegacyShortcutsPath  = `SOFTWARE\GoNhanh\Shortcuts`
	AppName              = "FKey"
)

// Settings keys
const (
	KeyInputMethod        = "InputMethod"
	KeyModernTone         = "ModernTone"
	KeyEnabled            = "Enabled"
	KeyFirstRun           = "FirstRun"
	KeyAutoStart          = "AutoStart"
	KeySkipWShortcut      = "SkipWShortcut"
	KeyEscRestore         = "EscRestore"
	KeyFreeTone           = "FreeTone"
	KeyEnglishAutoRestore = "EnglishAutoRestore"
	KeyAutoCapitalize     = "AutoCapitalize"
	KeyToggleHotkey       = "ToggleHotkey"
	KeyCoalescingApps     = "CoalescingApps"
	KeyShowOSD            = "ShowOSD"
	KeySmartPaste         = "SmartPaste"
	KeyRunAsAdmin         = "RunAsAdmin"
)

// Settings holds all application settings
type Settings struct {
	InputMethod        int    // 0=Telex, 1=VNI
	ModernTone         bool   // Modern tone placement
	Enabled            bool   // IME enabled
	FirstRun           bool   // First run flag
	AutoStart          bool   // Start with Windows
	SkipWShortcut      bool   // Skip w→ư in Telex
	EscRestore         bool   // ESC restores raw input
	FreeTone           bool   // Free tone placement
	EnglishAutoRestore bool   // Auto-restore English words
	AutoCapitalize     bool   // Auto-capitalize after punctuation
	ToggleHotkey       string // Format: "keycode,modifiers"
	CoalescingApps     string // Comma-separated list of apps
	ShowOSD            bool   // Show OSD when switching language
	SmartPaste         bool   // Smart paste (Ctrl+Shift+V fixes mojibake)
	RunAsAdmin         bool   // Run with administrator privileges
}

// DefaultSettings returns settings with default values
func DefaultSettings() *Settings {
	return &Settings{
		InputMethod:        0,       // Telex
		ModernTone:         true,
		Enabled:            true,
		FirstRun:           true,
		AutoStart:          false,
		SkipWShortcut:      false,
		EscRestore:         true,
		FreeTone:           false,
		EnglishAutoRestore: false,
		AutoCapitalize:     false,  // Default: OFF (user feedback)
		ToggleHotkey:       "0,5",  // Ctrl+Shift (modifier-only)
		CoalescingApps:     "discord,discordcanary,discordptb",
		ShowOSD:            false,  // Default: OFF
		SmartPaste:         true,   // Default: ON
		RunAsAdmin:         false,  // Default: OFF
	}
}

// SettingsService manages application settings via Registry
type SettingsService struct {
	settings *Settings
}

// NewSettingsService creates a new settings service
func NewSettingsService() *SettingsService {
	return &SettingsService{
		settings: DefaultSettings(),
	}
}

// Settings returns current settings
func (s *SettingsService) Settings() *Settings {
	return s.settings
}

// Load reads settings from Registry (tries FKey first, then legacy GoNhanh for migration)
func (s *SettingsService) Load() error {
	// Try new FKey key first
	key, err := registry.OpenKey(registry.CURRENT_USER, RegistryKeyPath, registry.QUERY_VALUE)
	if err != nil {
		// Try legacy GoNhanh key for migration
		key, err = registry.OpenKey(registry.CURRENT_USER, LegacyRegistryPath, registry.QUERY_VALUE)
		if err != nil {
			// No settings found, use defaults (first run)
			s.settings.FirstRun = true
			return nil
		}
	}
	defer key.Close()

	s.settings.InputMethod = readDWORD(key, KeyInputMethod, 0)
	s.settings.ModernTone = readDWORD(key, KeyModernTone, 1) == 1
	s.settings.Enabled = readDWORD(key, KeyEnabled, 1) == 1
	s.settings.FirstRun = readDWORD(key, KeyFirstRun, 1) == 1
	s.settings.AutoStart = readDWORD(key, KeyAutoStart, 0) == 1
	s.settings.SkipWShortcut = readDWORD(key, KeySkipWShortcut, 0) == 1
	s.settings.EscRestore = readDWORD(key, KeyEscRestore, 1) == 1
	s.settings.FreeTone = readDWORD(key, KeyFreeTone, 0) == 1
	s.settings.EnglishAutoRestore = readDWORD(key, KeyEnglishAutoRestore, 0) == 1
	s.settings.AutoCapitalize = readDWORD(key, KeyAutoCapitalize, 1) == 1
	s.settings.ToggleHotkey = readString(key, KeyToggleHotkey, "32,1")
	s.settings.CoalescingApps = readString(key, KeyCoalescingApps, "discord,discordcanary,discordptb")
	s.settings.ShowOSD = readDWORD(key, KeyShowOSD, 0) == 1
	s.settings.SmartPaste = readDWORD(key, KeySmartPaste, 1) == 1
	s.settings.RunAsAdmin = readDWORD(key, KeyRunAsAdmin, 0) == 1

	return nil
}

// Save writes settings to Registry
func (s *SettingsService) Save() error {
	key, _, err := registry.CreateKey(registry.CURRENT_USER, RegistryKeyPath, registry.SET_VALUE)
	if err != nil {
		return fmt.Errorf("failed to create registry key: %w", err)
	}
	defer key.Close()

	writeDWORD(key, KeyInputMethod, uint32(s.settings.InputMethod))
	writeDWORD(key, KeyModernTone, boolToDWORD(s.settings.ModernTone))
	writeDWORD(key, KeyEnabled, boolToDWORD(s.settings.Enabled))
	writeDWORD(key, KeyFirstRun, boolToDWORD(s.settings.FirstRun))
	writeDWORD(key, KeyAutoStart, boolToDWORD(s.settings.AutoStart))
	writeDWORD(key, KeySkipWShortcut, boolToDWORD(s.settings.SkipWShortcut))
	writeDWORD(key, KeyEscRestore, boolToDWORD(s.settings.EscRestore))
	writeDWORD(key, KeyFreeTone, boolToDWORD(s.settings.FreeTone))
	writeDWORD(key, KeyEnglishAutoRestore, boolToDWORD(s.settings.EnglishAutoRestore))
	writeDWORD(key, KeyAutoCapitalize, boolToDWORD(s.settings.AutoCapitalize))
	writeString(key, KeyToggleHotkey, s.settings.ToggleHotkey)
	writeString(key, KeyCoalescingApps, s.settings.CoalescingApps)
	writeDWORD(key, KeyShowOSD, boolToDWORD(s.settings.ShowOSD))
	writeDWORD(key, KeySmartPaste, boolToDWORD(s.settings.SmartPaste))
	writeDWORD(key, KeyRunAsAdmin, boolToDWORD(s.settings.RunAsAdmin))

	// Update auto-start registry
	s.updateAutoStart()

	return nil
}

// updateAutoStart updates Windows startup entry (registry or Task Scheduler)
func (s *SettingsService) updateAutoStart() {
	key, err := registry.OpenKey(registry.CURRENT_USER, AutoStartKeyPath, registry.SET_VALUE|registry.QUERY_VALUE)
	if err == nil {
		key.DeleteValue("GoNhanh")
		key.Close()
	}

	if s.settings.AutoStart && s.settings.RunAsAdmin {
		s.createScheduledTask()
		s.removeRegistryAutoStart()
	} else if s.settings.AutoStart {
		s.createRegistryAutoStart()
		s.removeScheduledTask()
	} else {
		s.removeRegistryAutoStart()
		s.removeScheduledTask()
	}
}

func (s *SettingsService) createRegistryAutoStart() {
	key, err := registry.OpenKey(registry.CURRENT_USER, AutoStartKeyPath, registry.SET_VALUE)
	if err != nil {
		return
	}
	defer key.Close()

	exePath, err := os.Executable()
	if err != nil {
		return
	}
	exePath, _ = filepath.EvalSymlinks(exePath)
	if exePath != "" {
		key.SetStringValue(AppName, fmt.Sprintf(`"%s"`, exePath))
	}
}

func (s *SettingsService) removeRegistryAutoStart() {
	key, err := registry.OpenKey(registry.CURRENT_USER, AutoStartKeyPath, registry.SET_VALUE)
	if err != nil {
		return
	}
	defer key.Close()
	key.DeleteValue(AppName)
}

func (s *SettingsService) createScheduledTask() {
	exePath, err := os.Executable()
	if err != nil {
		return
	}
	exePath, _ = filepath.EvalSymlinks(exePath)
	if exePath == "" {
		return
	}

	cmd := exec.Command("schtasks", "/Create",
		"/TN", AppName,
		"/TR", fmt.Sprintf(`"%s"`, exePath),
		"/SC", "ONLOGON",
		"/RL", "HIGHEST",
		"/F")
	cmd.SysProcAttr = &syscall.SysProcAttr{HideWindow: true}
	if err := cmd.Run(); err != nil {
		// Silently swallowing this used to mean a moved/renamed exe lost
		// autostart with no trace. Log so it's diagnosable.
		log.Printf("[Settings] schtasks /Create failed for %q: %v", exePath, err)
	}
}

func (s *SettingsService) removeScheduledTask() {
	cmd := exec.Command("schtasks", "/Delete",
		"/TN", AppName,
		"/F")
	cmd.SysProcAttr = &syscall.SysProcAttr{HideWindow: true}
	if err := cmd.Run(); err != nil {
		log.Printf("[Settings] schtasks /Delete failed: %v", err)
	}
}

// ReconcileScheduledTaskPath updates the scheduled task exe path if it exists
func (s *SettingsService) ReconcileScheduledTaskPath() {
	if !s.settings.AutoStart || !s.settings.RunAsAdmin {
		return
	}
	s.createScheduledTask()
}

// MarkFirstRunComplete sets FirstRun to false
func (s *SettingsService) MarkFirstRunComplete() {
	s.settings.FirstRun = false
	s.Save()
}

// Reset resets all settings to defaults
func (s *SettingsService) Reset() {
	s.settings = DefaultSettings()
	s.settings.FirstRun = false // Don't show onboarding again
	s.Save()
}

// GetCoalescingApps returns list of apps that benefit from coalescing
func (s *SettingsService) GetCoalescingApps() []string {
	if s.settings.CoalescingApps == "" {
		return nil
	}
	apps := strings.Split(s.settings.CoalescingApps, ",")
	for i, app := range apps {
		apps[i] = strings.TrimSpace(app)
	}
	return apps
}

// SetCoalescingApps updates the list
func (s *SettingsService) SetCoalescingApps(apps []string) {
	s.settings.CoalescingApps = strings.Join(apps, ",")
}

// Shortcut represents a text expansion shortcut
type Shortcut struct {
	Trigger     string
	Replacement string
	Enabled     bool
}

// LoadShortcuts reads shortcuts from Registry (tries FKey first, then legacy)
func (s *SettingsService) LoadShortcuts() ([]Shortcut, error) {
	// Try new FKey key first
	key, err := registry.OpenKey(registry.CURRENT_USER, ShortcutsKeyPath, registry.QUERY_VALUE)
	if err != nil {
		// Try legacy GoNhanh key
		key, err = registry.OpenKey(registry.CURRENT_USER, LegacyShortcutsPath, registry.QUERY_VALUE)
		if err != nil {
			return nil, nil // No shortcuts key yet
		}
	}
	defer key.Close()

	names, err := key.ReadValueNames(-1)
	if err != nil {
		return nil, err
	}

	shortcuts := make([]Shortcut, 0, len(names))
	for _, name := range names {
		value, _, err := key.GetStringValue(name)
		if err == nil {
			// Format: "replacement" or "replacement|0" (disabled) or "replacement|1" (enabled)
			replacement := value
			enabled := true
			if idx := strings.LastIndex(value, "|"); idx != -1 {
				replacement = value[:idx]
				enabled = value[idx+1:] != "0"
			}
			shortcuts = append(shortcuts, Shortcut{
				Trigger:     name,
				Replacement: replacement,
				Enabled:     enabled,
			})
		}
	}

	return shortcuts, nil
}

// SaveShortcuts writes shortcuts to Registry
func (s *SettingsService) SaveShortcuts(shortcuts []Shortcut) error {
	// Delete existing key and recreate
	registry.DeleteKey(registry.CURRENT_USER, ShortcutsKeyPath)

	if len(shortcuts) == 0 {
		return nil
	}

	key, _, err := registry.CreateKey(registry.CURRENT_USER, ShortcutsKeyPath, registry.SET_VALUE)
	if err != nil {
		return err
	}
	defer key.Close()

	for _, sc := range shortcuts {
		// Format: "replacement|1" (enabled) or "replacement|0" (disabled)
		enabledFlag := "1"
		if !sc.Enabled {
			enabledFlag = "0"
		}
		key.SetStringValue(sc.Trigger, sc.Replacement+"|"+enabledFlag)
	}

	return nil
}

// ParseHotkey parses "keycode,modifiers" format
func ParseHotkey(s string) (keyCode uint16, ctrl, alt, shift bool) {
	parts := strings.Split(s, ",")
	if len(parts) < 2 {
		return 0, false, false, false
	}

	kc, _ := strconv.ParseUint(parts[0], 10, 16)
	keyCode = uint16(kc)

	mod, _ := strconv.ParseUint(parts[1], 10, 8)
	ctrl = (mod & 1) != 0
	alt = (mod & 2) != 0
	shift = (mod & 4) != 0

	return
}

// FormatHotkey formats hotkey to "keycode,modifiers" string
func FormatHotkey(keyCode uint16, ctrl, alt, shift bool) string {
	mod := 0
	if ctrl {
		mod |= 1
	}
	if alt {
		mod |= 2
	}
	if shift {
		mod |= 4
	}
	return fmt.Sprintf("%d,%d", keyCode, mod)
}

// Helper functions

func readDWORD(key registry.Key, name string, defaultVal int) int {
	val, _, err := key.GetIntegerValue(name)
	if err != nil {
		return defaultVal
	}
	return int(val)
}

func readString(key registry.Key, name string, defaultVal string) string {
	val, _, err := key.GetStringValue(name)
	if err != nil {
		return defaultVal
	}
	return val
}

func writeDWORD(key registry.Key, name string, val uint32) {
	key.SetDWordValue(name, val)
}

func writeString(key registry.Key, name string, val string) {
	key.SetStringValue(name, val)
}

func boolToDWORD(b bool) uint32 {
	if b {
		return 1
	}
	return 0
}

// IsElevated returns whether the current process is running with administrator privileges
func IsElevated() bool {
	token := windows.GetCurrentProcessToken()
	return token.IsElevated()
}
