import Cocoa
import SwiftUI

// MARK: - Menu State

class MenuState: ObservableObject {
    static let shared = MenuState()

    @Published var isEnabled: Bool = true
    @Published var currentMethod: InputMode = .telex

    func toggle() {
        isEnabled.toggle()
        UserDefaults.standard.set(isEnabled, forKey: SettingsKey.enabled)
        RustBridge.setEnabled(isEnabled)
        NotificationCenter.default.post(name: .menuStateChanged, object: nil)
    }

    func setMethod(_ method: InputMode) {
        currentMethod = method
        UserDefaults.standard.set(method.rawValue, forKey: SettingsKey.method)
        RustBridge.setMethod(method.rawValue)
        NotificationCenter.default.post(name: .menuStateChanged, object: nil)
    }

    func load() {
        isEnabled = UserDefaults.standard.object(forKey: SettingsKey.enabled) as? Bool ?? true
        currentMethod = InputMode(rawValue: UserDefaults.standard.integer(forKey: SettingsKey.method)) ?? .telex
    }
}

extension Notification.Name {
    static let menuStateChanged = Notification.Name("menuStateChanged")
    static let showSettingsPage = Notification.Name("showSettingsPage")
}

// MARK: - Menu Popover

struct MenuPopoverView: View {
    @ObservedObject var state: MenuState
    let onClose: () -> Void

    @State private var shortcut = KeyboardShortcut.load()

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().padding(.horizontal, 8)
            methodSection
            Divider().padding(.horizontal, 8)
            actionSection
            Divider().padding(.horizontal, 8)
            quitSection
        }
        .frame(width: 240)
        .onReceive(NotificationCenter.default.publisher(for: .shortcutChanged)) { _ in
            shortcut = KeyboardShortcut.load()
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(nsImage: AppMetadata.logo)
                .resizable()
                .frame(width: 32, height: 32)

            VStack(alignment: .leading, spacing: 2) {
                Text(AppMetadata.name)
                    .font(.system(size: 13, weight: .semibold))
                HStack(spacing: 4) {
                    Text(state.isEnabled ? state.currentMethod.name : "Đã tắt")
                    Text("·").foregroundColor(Color(NSColor.tertiaryLabelColor))
                    Text(shortcut.displayParts.joined()).foregroundColor(Color(NSColor.tertiaryLabelColor))
                }
                .font(.system(size: 11))
                .foregroundColor(Color(NSColor.secondaryLabelColor))
            }

            Spacer()

            Toggle("", isOn: Binding(get: { state.isEnabled }, set: { _ in state.toggle() }))
                .toggleStyle(.switch)
                .labelsHidden()
                .scaleEffect(0.8)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
    }

    private var methodSection: some View {
        VStack(spacing: 0) {
            MenuItem(title: InputMode.telex.name, isChecked: state.currentMethod == .telex) {
                state.setMethod(.telex)
            }
            MenuItem(title: InputMode.vni.name, isChecked: state.currentMethod == .vni) {
                state.setMethod(.vni)
            }
        }
        .padding(.vertical, 4)
    }

    private var actionSection: some View {
        VStack(spacing: 0) {
            MenuItem(title: "Cài đặt...") {
                onClose()
                NotificationCenter.default.post(name: .showSettingsPage, object: NavigationPage.settings)
            }
            MenuItem(title: "Giới thiệu") {
                onClose()
                NotificationCenter.default.post(name: .showSettingsPage, object: NavigationPage.about)
            }
            MenuItem(title: "Kiểm tra cập nhật") {
                onClose()
                NotificationCenter.default.post(name: .showUpdateWindow, object: nil)
            }
        }
        .padding(.vertical, 4)
    }

    private var quitSection: some View {
        MenuItem(title: "Thoát \(AppMetadata.name)") {
            NSApp.terminate(nil)
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Menu Item

struct MenuItem: View {
    let title: String
    var isChecked: Bool = false
    let action: () -> Void

    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 0) {
                Text(isChecked ? "✓" : "")
                    .font(.system(size: 13))
                    .frame(width: 20, alignment: .center)
                Text(title)
                    .font(.system(size: 13))
                Spacer()
            }
            .foregroundColor(hovered ? .white : Color(NSColor.labelColor))
            .padding(.horizontal, 10)
            .padding(.vertical, 4)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 4).fill(hovered ? Color.accentColor : Color.clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 6)
        .onHover { hovered = $0 }
    }
}


// MARK: - Menu Bar Controller

class MenuBarController: NSObject, NSWindowDelegate {
    private var statusItem: NSStatusItem!

    private var onboardingWindow: NSWindow?
    private var updateWindow: NSWindow?
    private var settingsWindow: NSWindow?

    private let menuState = MenuState.shared

    override init() {
        super.init()
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

        setupMenu()
        setupNotifications()
        updateStatusButton()

        if UserDefaults.standard.bool(forKey: SettingsKey.hasCompletedOnboarding) && AXIsProcessTrusted() {
            loadSettings()
            startEngine()
        } else {
            showOnboarding()
        }
    }

    // MARK: - Setup

    private func setupNotifications() {
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(onboardingDidComplete),
            name: .onboardingCompleted,
            object: nil
        )

        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleToggleVietnamese),
            name: .toggleVietnamese,
            object: nil
        )

        NotificationCenter.default.addObserver(
            self,
            selector: #selector(checkForUpdates),
            name: .showUpdateWindow,
            object: nil
        )

        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleMenuStateChanged),
            name: .menuStateChanged,
            object: nil
        )

        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleShowSettingsPage),
            name: .showSettingsPage,
            object: nil
        )
    }

    @objc private func handleShowSettingsPage() {
        showSettings()
    }

    private func setupMenu() {
        let menu = NSMenu()

        // Header with toggle
        let header = NSMenuItem()
        header.view = createHeaderView()
        header.tag = 1
        menu.addItem(header)
        menu.addItem(.separator())

        // Input methods
        let telex = NSMenuItem(title: InputMode.telex.name, action: #selector(selectTelex), keyEquivalent: "")
        telex.target = self
        telex.tag = 10
        menu.addItem(telex)

        let vni = NSMenuItem(title: InputMode.vni.name, action: #selector(selectVNI), keyEquivalent: "")
        vni.target = self
        vni.tag = 11
        menu.addItem(vni)
        menu.addItem(.separator())

        // Settings
        let settings = NSMenuItem(title: "Cài đặt...", action: #selector(showSettings), keyEquivalent: "")
        settings.target = self
        menu.addItem(settings)

        // About
        let about = NSMenuItem(title: "Giới thiệu", action: #selector(showAbout), keyEquivalent: "")
        about.target = self
        menu.addItem(about)

        // Check for updates
        let checkUpdate = NSMenuItem(title: "Kiểm tra cập nhật", action: #selector(checkForUpdates), keyEquivalent: "")
        checkUpdate.target = self
        menu.addItem(checkUpdate)
        menu.addItem(.separator())

        // Quit
        let quit = NSMenuItem(title: "Thoát \(AppMetadata.name)", action: #selector(NSApp.terminate), keyEquivalent: "q")
        menu.addItem(quit)

        statusItem.menu = menu
        updateMenu()
    }

    private func createHeaderView() -> NSView {
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 220, height: 36))

        // App icon
        let iconView = NSImageView(frame: NSRect(x: 14, y: 4, width: 28, height: 28))
        iconView.image = AppMetadata.logo
        iconView.imageScaling = .scaleProportionallyUpOrDown
        view.addSubview(iconView)

        // App name + status
        let nameLabel = NSTextField(labelWithString: AppMetadata.name)
        nameLabel.font = .systemFont(ofSize: 13, weight: .semibold)
        nameLabel.frame = NSRect(x: 48, y: 16, width: 100, height: 16)
        view.addSubview(nameLabel)

        let shortcut = KeyboardShortcut.load()
        let statusText = menuState.isEnabled ? menuState.currentMethod.name : "Đã tắt"
        let statusLabel = NSTextField(labelWithString: "\(statusText) · \(shortcut.displayParts.joined())")
        statusLabel.font = .systemFont(ofSize: 11)
        statusLabel.textColor = .secondaryLabelColor
        statusLabel.frame = NSRect(x: 48, y: 2, width: 100, height: 14)
        statusLabel.tag = 100
        view.addSubview(statusLabel)

        // Toggle switch using SwiftUI
        let toggleView = NSHostingView(rootView:
            Toggle("", isOn: Binding(
                get: { [weak self] in self?.menuState.isEnabled ?? true },
                set: { [weak self] _ in self?.menuState.toggle() }
            ))
            .toggleStyle(.switch)
            .labelsHidden()
            .scaleEffect(0.8)
        )
        toggleView.frame = NSRect(x: 162, y: 4, width: 50, height: 28)
        view.addSubview(toggleView)

        return view
    }

    private func updateMenu() {
        guard let menu = statusItem.menu else { return }
        menu.item(withTag: 1)?.view = createHeaderView()
        menu.item(withTag: 10)?.state = menuState.currentMethod == .telex ? .on : .off
        menu.item(withTag: 11)?.state = menuState.currentMethod == .vni ? .on : .off
    }

    @objc private func selectTelex() { menuState.setMethod(.telex) }
    @objc private func selectVNI() { menuState.setMethod(.vni) }

    @objc private func showAbout() {
        showSettings()
        // Switch to About page after window is shown
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
            NotificationCenter.default.post(name: .showSettingsPage, object: NavigationPage.about)
        }
    }

    private func loadSettings() {
        menuState.load()
    }

    private func startEngine() {
        RustBridge.initialize()
        KeyboardHookManager.shared.start()
        RustBridge.setEnabled(menuState.isEnabled)
        RustBridge.setMethod(menuState.currentMethod.rawValue)

        // Sync shortcuts and excluded apps from AppState
        syncShortcutsToEngine()
        syncExcludedAppsToEngine()
        ExcludedAppsManager.shared.start()

        DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
            UpdateManager.shared.checkForUpdatesSilently()
        }
    }

    private func syncShortcutsToEngine() {
        let shortcuts = AppState.shared.shortcuts.map { ($0.key, $0.value, $0.isEnabled) }
        RustBridge.syncShortcuts(shortcuts)
    }

    private func syncExcludedAppsToEngine() {
        let bundleIds = AppState.shared.excludedApps
            .filter { $0.isEnabled }
            .map { $0.bundleId }
        ExcludedAppsManager.shared.setExcludedApps(bundleIds)
    }

    // MARK: - Status Button

    private func updateStatusButton() {
        guard let button = statusItem.button else { return }
        button.title = ""
        button.image = createStatusIcon(text: menuState.isEnabled ? "V" : "E")
    }

    private func createStatusIcon(text: String) -> NSImage {
        let width: CGFloat = 22
        let height: CGFloat = 16
        let image = NSImage(size: NSSize(width: width, height: height))

        image.lockFocus()

        let rect = NSRect(x: 0, y: 0, width: width, height: height)
        let path = NSBezierPath(roundedRect: rect, xRadius: 3, yRadius: 3)
        NSColor.white.setFill()
        path.fill()

        let font = NSFont.systemFont(ofSize: 13, weight: .bold)
        let attrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: NSColor.black
        ]
        let textSize = text.size(withAttributes: attrs)
        let textRect = NSRect(
            x: (width - textSize.width) / 2,
            y: (height - textSize.height) / 2,
            width: textSize.width,
            height: textSize.height
        )

        NSGraphicsContext.current?.compositingOperation = .destinationOut
        text.draw(in: textRect, withAttributes: attrs)

        image.unlockFocus()
        image.isTemplate = false
        return image
    }

    // MARK: - Event Handlers

    @objc private func handleToggleVietnamese() {
        menuState.toggle()
    }

    @objc private func handleMenuStateChanged() {
        menuState.load()
        updateStatusButton()
        updateMenu()
    }

    @objc private func onboardingDidComplete() {
        loadSettings()
        updateStatusButton()
        startEngine()
        enableLaunchAtLogin()
    }

    private func enableLaunchAtLogin() {
        do {
            try LaunchAtLoginManager.shared.enable()
        } catch {
            print("[LaunchAtLogin] Error: \(error)")
        }
    }

    // MARK: - Windows

    private func showOnboarding() {
        if onboardingWindow == nil {
            let view = OnboardingView()
            let controller = NSHostingController(rootView: view)
            onboardingWindow = NSWindow(contentViewController: controller)
            onboardingWindow?.title = AppMetadata.name
            onboardingWindow?.styleMask = [.titled, .closable]
            onboardingWindow?.setContentSize(controller.view.fittingSize)
            onboardingWindow?.center()
        }
        onboardingWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc private func showSettings() {
        if settingsWindow == nil {
            let controller = NSHostingController(rootView: MainSettingsView())
            controller.view.wantsLayer = true
            controller.view.layer?.backgroundColor = .clear
            let window = NSWindow(contentViewController: controller)
            window.title = "\(AppMetadata.name) - Cài đặt"
            window.styleMask = NSWindow.StyleMask([.titled, .closable, .miniaturizable, .fullSizeContentView])
            window.standardWindowButton(.zoomButton)?.isHidden = true
            window.setContentSize(NSSize(width: 700, height: 480))
            window.center()
            window.isReleasedWhenClosed = false
            window.titlebarAppearsTransparent = true
            window.titleVisibility = .hidden
            window.backgroundColor = .clear
            window.isOpaque = false
            window.hasShadow = true
            window.isMovableByWindowBackground = true
            window.delegate = self
            settingsWindow = window
        }
        // Show app in menu bar temporarily
        NSApp.setActivationPolicy(.regular)
        setupMainMenu()  // Set menu before showing window
        NSApp.activate(ignoringOtherApps: true)
        settingsWindow?.makeKeyAndOrderFront(nil)
        // Override default menu after window is shown (macOS may reset it)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
            self?.setupMainMenu()
        }
        // Clear auto-focus on TextFields
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
            self?.settingsWindow?.makeFirstResponder(nil)
        }
    }

    private func setupMainMenu() {
        // Clear existing menu first
        NSApp.mainMenu = nil
        let mainMenu = NSMenu()

        // App menu (shows app name in menu bar)
        let appMenu = NSMenu(title: AppMetadata.name)
        let appMenuItem = NSMenuItem(title: AppMetadata.name, action: nil, keyEquivalent: "")
        appMenuItem.submenu = appMenu

        // About
        let aboutItem = NSMenuItem(
            title: "Về \(AppMetadata.name)",
            action: #selector(showAbout),
            keyEquivalent: ""
        )
        aboutItem.target = self
        appMenu.addItem(aboutItem)

        appMenu.addItem(NSMenuItem.separator())

        // Check for updates
        let updateItem = NSMenuItem(
            title: "Kiểm tra cập nhật...",
            action: #selector(checkForUpdates),
            keyEquivalent: ""
        )
        updateItem.target = self
        appMenu.addItem(updateItem)

        appMenu.addItem(NSMenuItem.separator())

        // Settings
        let settingsItem = NSMenuItem(
            title: "Cài đặt...",
            action: #selector(showSettings),
            keyEquivalent: ""
        )
        settingsItem.target = self
        appMenu.addItem(settingsItem)

        appMenu.addItem(NSMenuItem.separator())

        // Quit (⌘Q)
        let quitItem = NSMenuItem(
            title: "Thoát \(AppMetadata.name)",
            action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q"
        )
        appMenu.addItem(quitItem)

        mainMenu.addItem(appMenuItem)

        // Edit menu (required for copy/paste in TextFields)
        let editMenu = NSMenu(title: "Sửa")
        let editMenuItem = NSMenuItem(title: "Sửa", action: nil, keyEquivalent: "")
        editMenuItem.submenu = editMenu

        editMenu.addItem(NSMenuItem(title: "Hoàn tác", action: Selector(("undo:")), keyEquivalent: "z"))
        editMenu.addItem(NSMenuItem(title: "Làm lại", action: Selector(("redo:")), keyEquivalent: "Z"))
        editMenu.addItem(NSMenuItem.separator())
        editMenu.addItem(NSMenuItem(title: "Cắt", action: #selector(NSText.cut(_:)), keyEquivalent: "x"))
        editMenu.addItem(NSMenuItem(title: "Sao chép", action: #selector(NSText.copy(_:)), keyEquivalent: "c"))
        editMenu.addItem(NSMenuItem(title: "Dán", action: #selector(NSText.paste(_:)), keyEquivalent: "v"))
        editMenu.addItem(NSMenuItem(title: "Chọn tất cả", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a"))

        mainMenu.addItem(editMenuItem)
        NSApp.mainMenu = mainMenu
    }

    @objc private func checkForUpdates() {
        if updateWindow == nil {
            let controller = NSHostingController(rootView: UpdateView())
            let window = NSWindow(contentViewController: controller)
            window.title = "Kiểm tra cập nhật"
            window.styleMask = [.titled, .closable]
            window.setContentSize(controller.view.fittingSize)
            window.center()
            window.isReleasedWhenClosed = false
            updateWindow = window
        }
        NSApp.activate(ignoringOtherApps: true)
        updateWindow?.makeKeyAndOrderFront(nil)

        // Skip re-check if update is already available (from auto-check)
        if case .available = UpdateManager.shared.state { return }
        UpdateManager.shared.checkForUpdatesManually()
    }

    // MARK: - NSWindowDelegate

    func windowWillClose(_ notification: Notification) {
        guard let window = notification.object as? NSWindow,
              window === settingsWindow else { return }
        // Revert to background app when settings window closes
        NSApp.setActivationPolicy(.accessory)
    }
}
