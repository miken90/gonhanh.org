import Cocoa
import SwiftUI

// MARK: - Menu Item Tags

private enum MenuTag: Int {
    case enabled = 100
    case telex = 200
    case vni = 201
    case status = 300
}

class MenuBarController {
    private var statusItem: NSStatusItem!
    private var settingsWindow: NSWindow?
    private var onboardingWindow: NSWindow?
    private var aboutWindow: NSWindow?
    private var isEnabled = true
    private var currentMethod: InputMode = .telex
    private var permissionGranted = false

    init() {
        // Load saved settings before UI setup
        loadSettings()

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        updateStatusButton()
        setupMenu()

        // Check if first launch or permission needed
        checkFirstLaunchOrPermission()

        // Listen for onboarding completion
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(onboardingDidComplete),
            name: .onboardingCompleted,
            object: nil
        )

        // Listen for settings changes
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(settingsDidChange),
            name: .settingsChanged,
            object: nil
        )
    }

    @objc private func settingsDidChange() {
        loadSettings()
        updateMenu()
    }

    private func loadSettings() {
        let defaults = UserDefaults.standard

        // Load enabled (default: true)
        if defaults.object(forKey: SettingsKey.enabled) != nil {
            isEnabled = defaults.bool(forKey: SettingsKey.enabled)
        }

        // Load method (default: 0=Telex)
        let methodValue = defaults.integer(forKey: SettingsKey.method)
        currentMethod = InputMode(rawValue: methodValue) ?? .telex
    }

    private func checkFirstLaunchOrPermission() {
        let defaults = UserDefaults.standard
        let hasCompletedOnboarding = defaults.bool(forKey: SettingsKey.hasCompletedOnboarding)
        let trusted = AXIsProcessTrusted()

        if !hasCompletedOnboarding || !trusted {
            // Show onboarding
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                self.showOnboarding()
            }
        } else {
            // Start keyboard hook directly
            startKeyboardHook()
        }
    }

    private func startKeyboardHook() {
        // Initialize Rust engine first (before setting any config)
        RustBridge.initialize()

        // Start keyboard hook (will skip init since already done)
        KeyboardHookManager.shared.start()

        // Apply settings AFTER engine is initialized
        RustBridge.setEnabled(isEnabled)
        RustBridge.setMethod(currentMethod.rawValue)

        permissionGranted = true
        updateMenu()
    }

    @objc private func onboardingDidComplete() {
        startKeyboardHook()
    }

    private func updateStatusButton() {
        guard let button = statusItem.button else { return }

        // Show method in title for quick reference
        if isEnabled {
            button.image = NSImage(systemSymbolName: "keyboard.fill", accessibilityDescription: AppMetadata.name)
            button.title = " \(currentMethod.shortName)"
        } else {
            button.image = NSImage(systemSymbolName: "keyboard", accessibilityDescription: AppMetadata.name)
            button.title = ""
        }

        // Adjust image position
        button.imagePosition = .imageLeading
    }

    private func setupMenu() {
        let menu = NSMenu()

        // Header with status
        let headerItem = NSMenuItem()
        headerItem.view = createHeaderView()
        menu.addItem(headerItem)

        menu.addItem(NSMenuItem.separator())

        // Enable/Disable with icon
        let enabledItem = NSMenuItem(
            title: "Bật GoNhanh",
            action: #selector(toggleEnabled),
            keyEquivalent: ""
        )
        enabledItem.target = self
        enabledItem.tag = MenuTag.enabled.rawValue
        enabledItem.state = isEnabled ? .on : .off
        menu.addItem(enabledItem)

        menu.addItem(NSMenuItem.separator())

        // Input method - direct items (not submenu for quicker access)
        let methodLabel = NSMenuItem(title: "Kiểu gõ:", action: nil, keyEquivalent: "")
        methodLabel.isEnabled = false
        menu.addItem(methodLabel)

        let telexItem = NSMenuItem(title: "   Telex", action: #selector(setTelex), keyEquivalent: "t")
        telexItem.keyEquivalentModifierMask = [.command, .shift]
        telexItem.target = self
        telexItem.tag = MenuTag.telex.rawValue
        telexItem.state = currentMethod == .telex ? .on : .off
        menu.addItem(telexItem)

        let vniItem = NSMenuItem(title: "   VNI", action: #selector(setVNI), keyEquivalent: "v")
        vniItem.keyEquivalentModifierMask = [.command, .shift]
        vniItem.target = self
        vniItem.tag = MenuTag.vni.rawValue
        vniItem.state = currentMethod == .vni ? .on : .off
        menu.addItem(vniItem)

        menu.addItem(NSMenuItem.separator())

        // Quick actions
        let settingsItem = NSMenuItem(
            title: "Cài đặt...",
            action: #selector(openSettings),
            keyEquivalent: ","
        )
        settingsItem.target = self
        menu.addItem(settingsItem)

        let aboutItem = NSMenuItem(
            title: "Về \(AppMetadata.name)",
            action: #selector(showAbout),
            keyEquivalent: ""
        )
        aboutItem.target = self
        menu.addItem(aboutItem)

        // Help/Feedback
        let helpItem = NSMenuItem(
            title: "Trợ giúp & Góp ý",
            action: #selector(openHelp),
            keyEquivalent: "?"
        )
        helpItem.target = self
        menu.addItem(helpItem)

        menu.addItem(NSMenuItem.separator())

        let quitItem = NSMenuItem(
            title: "Thoát",
            action: #selector(quit),
            keyEquivalent: "q"
        )
        quitItem.target = self
        menu.addItem(quitItem)

        statusItem.menu = menu
    }

    private func createHeaderView() -> NSView {
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 220, height: 50))

        // App name
        let titleLabel = NSTextField(labelWithString: AppMetadata.name)
        titleLabel.font = NSFont.systemFont(ofSize: 14, weight: .semibold)
        titleLabel.frame = NSRect(x: 14, y: 26, width: 150, height: 18)
        view.addSubview(titleLabel)

        // Status
        let statusText = isEnabled ? "Đang bật • \(currentMethod.name)" : "Đang tắt"
        let statusLabel = NSTextField(labelWithString: statusText)
        statusLabel.font = NSFont.systemFont(ofSize: 11)
        statusLabel.textColor = isEnabled ? .systemGreen : .secondaryLabelColor
        statusLabel.frame = NSRect(x: 14, y: 8, width: 150, height: 14)
        statusLabel.tag = MenuTag.status.rawValue
        view.addSubview(statusLabel)

        // Version badge
        let versionLabel = NSTextField(labelWithString: "v\(AppMetadata.version)")
        versionLabel.font = NSFont.systemFont(ofSize: 10)
        versionLabel.textColor = .tertiaryLabelColor
        versionLabel.alignment = .right
        versionLabel.frame = NSRect(x: 160, y: 26, width: 50, height: 14)
        view.addSubview(versionLabel)

        return view
    }

    private func updateMenu() {
        guard let menu = statusItem.menu else { return }

        // Update header view
        if let headerItem = menu.items.first, headerItem.view != nil {
            headerItem.view = createHeaderView()
        }

        // Update enabled state
        if let item = menu.item(withTag: MenuTag.enabled.rawValue) {
            item.state = isEnabled ? .on : .off
        }

        // Update method checkmarks
        menu.item(withTag: MenuTag.telex.rawValue)?.state = currentMethod == .telex ? .on : .off
        menu.item(withTag: MenuTag.vni.rawValue)?.state = currentMethod == .vni ? .on : .off

        // Update status button
        updateStatusButton()
    }

    // MARK: - Actions

    @objc func toggleEnabled() {
        isEnabled.toggle()
        RustBridge.setEnabled(isEnabled)
        UserDefaults.standard.set(isEnabled, forKey: SettingsKey.enabled)
        updateMenu()
    }

    @objc func setTelex() {
        setMethod(.telex)
    }

    @objc func setVNI() {
        setMethod(.vni)
    }

    private func setMethod(_ mode: InputMode) {
        currentMethod = mode
        RustBridge.setMethod(mode.rawValue)
        UserDefaults.standard.set(mode.rawValue, forKey: SettingsKey.method)
        updateMenu()
    }

    @objc func openSettings() {
        if settingsWindow == nil {
            let contentView = SettingsView()
            let hostingController = NSHostingController(rootView: contentView)

            settingsWindow = NSWindow(contentViewController: hostingController)
            settingsWindow?.title = "\(AppMetadata.name) - Cài đặt"
            settingsWindow?.styleMask = [.titled, .closable]
            settingsWindow?.setContentSize(NSSize(width: 400, height: 320))
            settingsWindow?.center()
        }

        settingsWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc func showOnboarding() {
        if onboardingWindow == nil {
            let contentView = OnboardingView()
            let hostingController = NSHostingController(rootView: contentView)

            onboardingWindow = NSWindow(contentViewController: hostingController)
            onboardingWindow?.title = "Chào mừng đến với \(AppMetadata.name)"
            onboardingWindow?.styleMask = [.titled, .closable]
            onboardingWindow?.setContentSize(NSSize(width: 500, height: 400))
            onboardingWindow?.center()
        }

        onboardingWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc func showAbout() {
        if aboutWindow == nil {
            let contentView = AboutView()
            let hostingController = NSHostingController(rootView: contentView)

            aboutWindow = NSWindow(contentViewController: hostingController)
            aboutWindow?.title = "Về \(AppMetadata.name)"
            aboutWindow?.styleMask = [.titled, .closable]
            aboutWindow?.setContentSize(NSSize(width: 340, height: 380))
            aboutWindow?.center()
        }

        aboutWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc func openHelp() {
        if let url = URL(string: AppMetadata.issuesURL) {
            NSWorkspace.shared.open(url)
        }
    }

    @objc func quit() {
        NSApp.terminate(nil)
    }
}
