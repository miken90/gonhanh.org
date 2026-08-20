package main

// Updater dialog flow: background update check at startup and the
// download/install/relaunch sequence when the user opts in. Moved out of
// main.go to keep it focused on process startup.

import (
	"fmt"
	"log"
	"time"
)

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
