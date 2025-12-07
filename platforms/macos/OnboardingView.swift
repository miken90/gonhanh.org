import SwiftUI

// MARK: - Onboarding View

struct OnboardingView: View {
    @State private var currentStep = 0
    @State private var hasPermission = false
    @State private var selectedMode: InputMode = .telex

    private let timer = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    // Đã restart sau khi cấp quyền?
    private var isPostRestart: Bool {
        UserDefaults.standard.bool(forKey: "gonhanh.didRestart")
    }

    var body: some View {
        VStack(spacing: 0) {
            contentView.frame(height: 340)
            Divider()
            footerView
        }
        .frame(width: 480)
        .onAppear {
            hasPermission = AXIsProcessTrusted()
            // Nếu đã restart và có quyền -> bắt đầu từ Success
            if isPostRestart && hasPermission {
                currentStep = 10  // Success step
            }
        }
        .onReceive(timer) { _ in
            hasPermission = AXIsProcessTrusted()
        }
    }

    // MARK: - Content

    @ViewBuilder
    private var contentView: some View {
        switch currentStep {
        case 0:
            WelcomeView()
        case 1:
            PermissionView(hasPermission: hasPermission)
        case 10:  // Post-restart: Success
            SuccessView()
        case 11:  // Post-restart: Setup
            SetupView(selectedMode: $selectedMode)
        default:
            EmptyView()
        }
    }

    // MARK: - Footer

    private var footerView: some View {
        HStack {
            // Step indicator
            HStack(spacing: 6) {
                let steps = isPostRestart ? 2 : 2
                let current = isPostRestart ? (currentStep - 10) : currentStep
                ForEach(0..<steps, id: \.self) { i in
                    Circle()
                        .fill(i == current ? Color.accentColor : Color.secondary.opacity(0.3))
                        .frame(width: 6, height: 6)
                }
            }

            Spacer()

            // Buttons
            HStack(spacing: 12) {
                if currentStep == 1 && !hasPermission {
                    // Quay lại từ Permission
                    Button("Quay lại") { currentStep = 0 }
                }
                actionButton
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
    }

    @ViewBuilder
    private var actionButton: some View {
        switch currentStep {
        case 0:
            Button("Tiếp tục") { currentStep = 1 }
                .buttonStyle(.borderedProminent)

        case 1:
            if hasPermission {
                // Đã cấp quyền -> cần restart
                Button("Khởi động lại") { restartApp() }
                    .buttonStyle(.borderedProminent)
            } else {
                Button("Mở Cài đặt") { openSettings() }
                    .buttonStyle(.borderedProminent)
            }

        case 10:  // Success
            Button("Tiếp tục") { currentStep = 11 }
                .buttonStyle(.borderedProminent)

        case 11:  // Setup
            Button("Hoàn tất") { finish() }
                .buttonStyle(.borderedProminent)

        default:
            EmptyView()
        }
    }

    // MARK: - Actions

    private func openSettings() {
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!)
    }

    private func restartApp() {
        UserDefaults.standard.set(true, forKey: "gonhanh.didRestart")
        UserDefaults.standard.set(selectedMode.rawValue, forKey: SettingsKey.method)

        let path = Bundle.main.bundlePath
        // Đợi UserDefaults sync xong trước khi terminate
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
            let task = Process()
            task.launchPath = "/bin/sh"
            task.arguments = ["-c", "sleep 0.3 && open \"\(path)\""]
            try? task.run()
            NSApp.terminate(nil)
        }
    }

    private func finish() {
        UserDefaults.standard.set(selectedMode.rawValue, forKey: SettingsKey.method)
        UserDefaults.standard.set(true, forKey: SettingsKey.hasCompletedOnboarding)
        UserDefaults.standard.removeObject(forKey: "gonhanh.didRestart")

        // Chuyển sang accessory mode (ẩn dock icon)
        NSApp.setActivationPolicy(.accessory)

        NotificationCenter.default.post(name: .onboardingCompleted, object: nil)
        NSApp.keyWindow?.close()
    }
}

// MARK: - Pages

private struct WelcomeView: View {
    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(nsImage: AppMetadata.logo)
                .resizable()
                .frame(width: 80, height: 80)
            Text("Chào mừng đến với \(AppMetadata.name)")
                .font(.system(size: 22, weight: .bold))
            Text(AppMetadata.tagline)
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(.horizontal, 40)
    }
}

private struct PermissionView: View {
    let hasPermission: Bool

    var body: some View {
        VStack(spacing: 16) {
            Spacer()

            Image(systemName: hasPermission ? "checkmark.shield.fill" : "hand.raised.fill")
                .font(.system(size: 40))
                .foregroundStyle(hasPermission ? .green : .orange)

            Text(hasPermission ? "Đã cấp quyền" : "Cấp quyền Accessibility")
                .font(.system(size: 22, weight: .bold))

            if hasPermission {
                Text("Nhấn \"Khởi động lại\" để áp dụng.")
                    .foregroundStyle(.secondary)
            } else {
                Text("Bật \(AppMetadata.name) trong System Settings để gõ tiếng Việt.")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)

                VStack(alignment: .leading, spacing: 8) {
                    Label("Mở Privacy & Security → Accessibility", systemImage: "1.circle.fill")
                    Label("Bật công tắc bên cạnh \(AppMetadata.name)", systemImage: "2.circle.fill")
                }
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding(.top, 8)
            }

            Spacer()
        }
        .padding(.horizontal, 40)
    }
}

private struct SuccessView: View {
    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 48))
                .foregroundStyle(.green)
            Text("Sẵn sàng hoạt động")
                .font(.system(size: 22, weight: .bold))
            Text("\(AppMetadata.name) đã được cấp quyền thành công.")
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(.horizontal, 40)
    }
}

private struct SetupView: View {
    @Binding var selectedMode: InputMode

    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "keyboard")
                .font(.system(size: 40))
                .foregroundStyle(.blue)
            Text("Chọn kiểu gõ")
                .font(.system(size: 22, weight: .bold))
            Text("Có thể thay đổi sau trong menu.")
                .foregroundStyle(.secondary)

            VStack(spacing: 8) {
                ForEach(InputMode.allCases, id: \.rawValue) { mode in
                    ModeButton(mode: mode, isSelected: selectedMode == mode) {
                        selectedMode = mode
                    }
                }
            }
            .frame(maxWidth: 260)
            .padding(.top, 8)
            Spacer()
        }
        .padding(.horizontal, 40)
    }
}

private struct ModeButton: View {
    let mode: InputMode
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(mode.name).font(.headline)
                    Text(mode.description).font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Image(systemName: isSelected ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(isSelected ? .blue : .secondary.opacity(0.4))
            }
            .padding(10)
            .background(RoundedRectangle(cornerRadius: 8).fill(isSelected ? Color.blue.opacity(0.1) : Color.secondary.opacity(0.05)))
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(isSelected ? Color.blue.opacity(0.5) : .clear))
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Notification

extension Notification.Name {
    static let onboardingCompleted = Notification.Name("onboardingCompleted")
}
