package main

import (
	"fmt"
	"io/fs"
	"log"
	"os"
	"syscall"
	"time"
	"unsafe"

	"fkey/core"
	"fkey/services"

	"github.com/wailsapp/wails/v3/pkg/application"
	"github.com/wailsapp/wails/v3/pkg/events"
)

// Windows MessageBox constants
const (
	MB_OK              = 0x00000000
	MB_OKCANCEL        = 0x00000001
	MB_YESNO           = 0x00000004
	MB_YESNOCANCEL     = 0x00000003
	MB_ICONINFORMATION = 0x00000040
	MB_ICONWARNING     = 0x00000030
	MB_ICONQUESTION    = 0x00000020
	IDYES              = 6
	IDNO               = 7
	IDOK               = 1
	IDCANCEL           = 2
)

var (
	user32DLL       = syscall.NewLazyDLL("user32.dll")
	procMessageBoxW = user32DLL.NewProc("MessageBoxW")
)

// showMessageBox shows a Windows message box
func showMessageBox(title, message string, flags uint32) int {
	titlePtr, _ := syscall.UTF16PtrFromString(title)
	messagePtr, _ := syscall.UTF16PtrFromString(message)
	ret, _, _ := procMessageBoxW.Call(
		0,
		uintptr(unsafe.Pointer(messagePtr)),
		uintptr(unsafe.Pointer(titlePtr)),
		uintptr(flags),
	)
	return int(ret)
}

// FKey - Vietnamese Input Method
// Wails v3 implementation (target: ~5MB)

// Version is set via -ldflags at build time: -X main.Version=x.x.x
var Version = "dev"

// Icons generated at runtime
var (
	iconOn  []byte
	iconOff []byte
)

// Global references for updates
var (
	globalApp         *application.App
	globalTray        *application.SystemTray
	globalMenu        *application.Menu
	globalImeLoop     *core.ImeLoop
	globalSettingsWin application.Window
	settingsSvc       *services.SettingsService
	updaterSvc        *services.UpdaterService
	formattingSvc     *services.FormattingService
	wantQuit          bool // Flag to allow quit via tray menu
)

func main() {
	core.StartupTraceBegin()

	relaunch := false
	for _, arg := range os.Args[1:] {
		if arg == "--relaunch" {
			relaunch = true
			break
		}
	}

	if err := core.AcquireMutex(relaunch); err != nil {
		showMessageBox("FKey", err.Error(), MB_OK|MB_ICONWARNING)
		os.Exit(1)
	}
	defer core.ReleaseMutex()
	core.StartupTraceStage("mutex")

	core.QuitApp = func() {
		wantQuit = true
		globalApp.Quit()
	}

	core.RevertRunAsAdmin = func() {
		if settingsSvc != nil {
			settingsSvc.Settings().RunAsAdmin = false
			settingsSvc.Save()
		}
	}

	// Extract embedded DLL (single-exe distribution)
	dllPath, err := GetDLLPath()
	if err != nil {
		log.Fatalf("Failed to extract DLL: %v", err)
	}
	core.DLLPath = dllPath
	log.Printf("Using DLL: %s", dllPath)
	core.StartupTraceStage("dll-extract")

	// Generate icons
	iconOn = CreateIconOn()
	iconOff = CreateIconOff()
	core.StartupTraceStage("icons")

	// Initialize services
	settingsSvc = services.NewSettingsService()
	if err := settingsSvc.Load(); err != nil {
		log.Printf("Failed to load settings: %v", err)
	}
	settings := settingsSvc.Settings()
	core.StartupTraceStage("settings-load")

	if settings.RunAsAdmin && !services.IsElevated() {
		log.Printf("RunAsAdmin enabled but not elevated, re-launching...")
		core.ElevateAndRelaunch()
		return
	}

	if settings.AutoStart && settings.RunAsAdmin && services.IsElevated() {
		// Off the boot path: this shells out to schtasks.exe, which has no
		// business blocking typing-readiness. Only affects RunAsAdmin users.
		go func() {
			time.Sleep(3 * time.Second)
			settingsSvc.ReconcileScheduledTaskPath()
		}()
	}

	// Initialize formatting service
	formattingSvc = services.NewFormattingService()
	if err := formattingSvc.Load(); err != nil {
		log.Printf("Failed to load formatting config: %v", err)
	}
	core.StartupTraceStage("formatting-load")

	// Initialize IME loop
	globalImeLoop, err = core.NewImeLoop()
	if err != nil {
		log.Fatalf("Failed to create IME loop: %v", err)
	}
	core.StartupTraceStage("imeloop-create")

	// Apply settings to IME
	applySettings(globalImeLoop, settings)

	// Initialize format handler for text formatting feature
	core.InitFormatHandler(formattingSvc)

	// Create App bindings
	appBindings := NewAppBindings(globalImeLoop, settingsSvc, formattingSvc)

	// Create embedded assets filesystem
	frontendFS, err := fs.Sub(assets, "frontend")
	if err != nil {
		log.Fatalf("Failed to create frontend filesystem: %v", err)
	}

	// Create Wails application with bundled asset server (injects runtime automatically)
	globalApp = application.New(application.Options{
		Name:        "FKey",
		Description: "Vietnamese Input Method",
		Icon:        iconOn, // Application icon for windows
		Assets: application.AssetOptions{
			Handler: application.BundledAssetFileServer(frontendFS),
		},
		Services: []application.Service{
			application.NewService(appBindings), // Pass pointer directly
		},
		// Prevent app from quitting when settings window closes
		// App should only quit via tray menu "Thoát"
		ShouldQuit: func() bool {
			return wantQuit
		},
		Windows: application.WindowsOptions{
			// Windows-specific options
		},
	})
	core.StartupTraceStage("wails-new")

	// Create system tray
	globalTray = globalApp.SystemTray.New()
	if settings.Enabled {
		globalTray.SetIcon(iconOn)
		globalTray.SetTooltip("FKey - Tiếng Việt (Bật)")
	} else {
		globalTray.SetIcon(iconOff)
		globalTray.SetTooltip("FKey - Tiếng Việt (Tắt)")
	}

	// Settings window created on-demand (lazy-load for RAM optimization)
	// globalSettingsWin starts as nil - created when user opens settings

	// Create tray menu
	globalMenu = createTrayMenu(settings.Enabled)
	globalTray.SetMenu(globalMenu)

	// Left-click toggles IME
	globalTray.OnClick(func() {
		toggleIME()
	})

	// Status callback - called when hotkey toggles IME
	globalImeLoop.OnEnabledChanged = func(enabled bool) {
		updateUI(enabled)
		// Play beep sound when toggled via hotkey
		core.PlayBeep(enabled)
	}
	core.StartupTraceStage("tray-ready")

	// Start IME loop BEFORE app.Run() so keyboard hook is active
	if err := globalImeLoop.Start(); err != nil {
		log.Fatalf("Failed to start IME loop: %v", err)
	}
	core.StartupTraceStage("hook-start")

	// Initialize updater service
	updaterSvc = services.NewUpdaterService(Version)

	// Check for updates in background (non-blocking)
	go checkForUpdatesBackground()

	log.Printf("FKey started. IME: %s, Method: %d",
		map[bool]string{true: "ON", false: "OFF"}[settings.Enabled],
		settings.InputMethod)

	core.StartupTraceStage("run")
	core.StartupTraceFinish()

	// Run application (blocks until quit)
	if err := globalApp.Run(); err != nil {
		log.Fatal(err)
	}

	// Cleanup
	globalImeLoop.Stop()
}

// checkForUpdatesBackground checks for updates silently at startup
func checkForUpdatesBackground() {
	// Wait a bit for app to fully initialize
	time.Sleep(3 * time.Second)

	info, err := updaterSvc.CheckForUpdates(false)
	if err != nil {
		log.Printf("Update check failed: %v", err)
		return
	}

	if info.Available && info.DownloadURL != "" {
		log.Printf("Update available: %s -> %s", info.CurrentVersion, info.LatestVersion)
		// Show update notification dialog with auto-update option
		result := showMessageBox("Có phiên bản mới!",
			fmt.Sprintf("Phiên bản mới: %s\nPhiên bản hiện tại: %s\n\nBạn có muốn tự động cập nhật?\n\n(Chọn No để mở trang tải về)",
				info.LatestVersion, info.CurrentVersion),
			MB_YESNOCANCEL|MB_ICONINFORMATION)
		if result == IDYES {
			performAutoUpdate(info.DownloadURL)
		} else if result == IDNO {
			updaterSvc.OpenReleasePage(info.ReleaseURL)
		}
	}
}

// performAutoUpdate downloads and installs update automatically
func performAutoUpdate(downloadURL string) {
	log.Printf("Starting auto-update from: %s", downloadURL)
	
	// Show downloading message
	showMessageBox("Cập nhật FKey", 
		"FKey sẽ tải bản cập nhật sau khi click OK.\nNhấn OK để tiếp tục...", 
		MB_OK|MB_ICONINFORMATION)
	
	// Download update
	zipPath, err := updaterSvc.DownloadUpdate(downloadURL, nil)
	if err != nil {
		log.Printf("Download failed: %v", err)
		showMessageBox("Lỗi cập nhật", 
			"Không thể tải bản cập nhật.\n\n"+err.Error(), 
			MB_OK|MB_ICONWARNING)
		return
	}
	log.Printf("Downloaded to: %s", zipPath)
	
	// Install update (creates batch script)
	batchPath, err := updaterSvc.InstallUpdate(zipPath)
	if err != nil {
		log.Printf("Install failed: %v", err)
		showMessageBox("Lỗi cập nhật", 
			"Không thể cài đặt bản cập nhật.\n\n"+err.Error(), 
			MB_OK|MB_ICONWARNING)
		return
	}
	log.Printf("Update script created: %s", batchPath)
	
	// Run update script and quit app
	if err := updaterSvc.RunUpdateScript(batchPath); err != nil {
		log.Printf("Failed to run update script: %v", err)
		showMessageBox("Lỗi cập nhật", 
			"Không thể chạy script cập nhật.\n\n"+err.Error(), 
			MB_OK|MB_ICONWARNING)
		return
	}
	
	// Quit app to allow update
	log.Printf("Quitting for update...")
	wantQuit = true
	globalApp.Quit()
}

func toggleIME() {
	enabled := globalImeLoop.Toggle()
	settingsSvc.Settings().Enabled = enabled
	settingsSvc.Save()
	updateUI(enabled)
	// Play beep sound to indicate toggle
	core.PlayBeep(enabled)
}

// showOSDPopup displays a brief on-screen notification when switching language
func showOSDPopup(isVietnamese bool) {
	var title, message string
	if isVietnamese {
		title = "FKey"
		message = "🇻🇳 Tiếng Việt"
	} else {
		title = "FKey"
		message = "🇺🇸 English"
	}
	
	// Use Windows MessageBox with auto-close via timer
	// For non-blocking: spawn a goroutine that shows a quick tooltip-style message
	time.Sleep(100 * time.Millisecond) // Brief delay to avoid UI race
	
	// Create a simple tooltip-style window using MessageBox with timeout
	// Note: This is a temporary solution. Proper OSD would use layered windows.
	showTooltipNotification(title, message)
}

// showSettingsWindow creates settings window on-demand (lazy-load) and hides on close
// WebView2 stays in RAM after first open, but this prevents app crash
func showSettingsWindow() {
	if globalSettingsWin == nil {
		globalSettingsWin = globalApp.Window.NewWithOptions(application.WebviewWindowOptions{
			Name:                       "FKey Settings",
			Title:                      "FKey - Cài đặt",
			Width:                      520,
			Height:                     560,
			Hidden:                     false, // Show immediately
			DisableResize:              false,
			URL:                        "/",
			DevToolsEnabled:            false,
			DefaultContextMenuDisabled: true,
			Windows: application.WindowsWindow{
				HiddenOnTaskbar: false,
			},
		})

		// Hide window on close instead of destroying
		// Wails v3 quits app when last window closes, even with ShouldQuit
		globalSettingsWin.RegisterHook(events.Common.WindowClosing, func(e *application.WindowEvent) {
			globalSettingsWin.Hide()
			e.Cancel()
		})
	}
	globalSettingsWin.Show()
	globalSettingsWin.Focus()
}

// showTooltipNotification shows a brief tooltip notification
func showTooltipNotification(title, message string) {
	// Update the tooltip temporarily to show language change
	// The tooltip will be shown when user hovers over the tray icon
	globalTray.SetTooltip(message)
	
	// Restore normal tooltip after a delay
	go func() {
		time.Sleep(2 * time.Second)
		if settingsSvc.Settings().Enabled {
			globalTray.SetTooltip("FKey - Tiếng Việt (Bật)")
		} else {
			globalTray.SetTooltip("FKey - Tiếng Việt (Tắt)")
		}
	}()
}

func updateUI(enabled bool) {
	// Update tray icon
	if enabled {
		globalTray.SetIcon(iconOn)
		globalTray.SetTooltip("FKey - Tiếng Việt (Bật)")
	} else {
		globalTray.SetIcon(iconOff)
		globalTray.SetTooltip("FKey - Tiếng Việt (Tắt)")
	}

	// Emit event to frontend so Settings UI can update status indicator
	globalApp.Event.Emit("ime:status-changed", enabled)

	// Show OSD popup if enabled
	if settingsSvc.Settings().ShowOSD {
		go showOSDPopup(enabled)
	}

	// Rebuild menu with new state
	globalMenu = createTrayMenu(enabled)
	globalTray.SetMenu(globalMenu)
}

func applySettings(loop *core.ImeLoop, settings *services.Settings) {
	imeSettings := &core.ImeSettings{
		Enabled:            settings.Enabled,
		InputMethod:        core.InputMethod(settings.InputMethod),
		ModernTone:         settings.ModernTone,
		SkipWShortcut:      settings.SkipWShortcut,
		EscRestore:         settings.EscRestore,
		FreeTone:           settings.FreeTone,
		EnglishAutoRestore: settings.EnglishAutoRestore,
		AutoCapitalize:     settings.AutoCapitalize,
	}
	loop.UpdateSettings(imeSettings)

	// Set hotkey
	keyCode, ctrl, alt, shift := services.ParseHotkey(settings.ToggleHotkey)
	loop.SetHotkey(keyCode, ctrl, alt, shift)

	// Set SmartPaste enabled state
	core.SetSmartPasteEnabled(settings.SmartPaste)

	// Load shortcuts
	shortcuts, err := settingsSvc.LoadShortcuts()
	if err == nil {
		for _, sc := range shortcuts {
			if sc.Enabled {
				loop.AddShortcut(sc.Trigger, sc.Replacement)
			}
		}
	}
}

func createTrayMenu(enabled bool) *application.Menu {
	menu := globalApp.NewMenu()
	settings := settingsSvc.Settings()

	// Status indicator with checkbox
	enabledItem := menu.AddCheckbox("Tiếng Việt", enabled)
	enabledItem.OnClick(func(ctx *application.Context) {
		toggleIME()
	})

	menu.AddSeparator()

	// Input method
	methodMenu := menu.AddSubmenu("Kiểu gõ")
	telexItem := methodMenu.AddRadio("Telex", settings.InputMethod == 0)
	vniItem := methodMenu.AddRadio("VNI", settings.InputMethod == 1)

	telexItem.OnClick(func(ctx *application.Context) {
		globalImeLoop.UpdateSettings(&core.ImeSettings{
			Enabled:     settingsSvc.Settings().Enabled,
			InputMethod: core.Telex,
		})
		settingsSvc.Settings().InputMethod = 0
		settingsSvc.Save()
	})
	vniItem.OnClick(func(ctx *application.Context) {
		globalImeLoop.UpdateSettings(&core.ImeSettings{
			Enabled:     settingsSvc.Settings().Enabled,
			InputMethod: core.VNI,
		})
		settingsSvc.Settings().InputMethod = 1
		settingsSvc.Save()
	})

	menu.AddSeparator()

	// Settings
	menu.Add("Cài đặt...").OnClick(func(ctx *application.Context) {
		showSettingsWindow()
	})

	// Check for updates
	menu.Add("Kiểm tra cập nhật...").OnClick(func(ctx *application.Context) {
		go func() {
			info, err := updaterSvc.CheckForUpdates(true)
			if err != nil {
				log.Printf("Update check failed: %v", err)
				showMessageBox("Kiểm tra cập nhật", 
					"Không thể kiểm tra cập nhật.\n\n"+err.Error(), 
					MB_OK|MB_ICONWARNING)
				return
			}
			if info.Available && info.DownloadURL != "" {
				log.Printf("Update available: %s", info.LatestVersion)
				result := showMessageBox("Có phiên bản mới!", 
					fmt.Sprintf("Phiên bản mới: %s\nPhiên bản hiện tại: %s\n\nBạn có muốn tự động cập nhật?\n\n(Chọn No để mở trang tải về)", 
						info.LatestVersion, info.CurrentVersion),
					MB_YESNOCANCEL|MB_ICONQUESTION)
				if result == IDYES {
					performAutoUpdate(info.DownloadURL)
				} else if result == IDNO {
					updaterSvc.OpenReleasePage(info.ReleaseURL)
				}
			} else {
				log.Printf("Already at latest version: %s", info.CurrentVersion)
				showMessageBox("Kiểm tra cập nhật", 
					fmt.Sprintf("Bạn đang sử dụng phiên bản mới nhất.\n\nPhiên bản: %s", info.CurrentVersion),
					MB_OK|MB_ICONINFORMATION)
			}
		}()
	})

	menu.AddSeparator()

	// Version
	menu.Add(fmt.Sprintf("FKey v%s", Version)).SetEnabled(false)

	menu.AddSeparator()

	// Quit
	menu.Add("Thoát").OnClick(func(ctx *application.Context) {
		wantQuit = true
		globalApp.Quit()
	})

	return menu
}
