import SwiftUI

// MARK: - Onboarding View

struct OnboardingView: View {
    @State private var currentStep = 0
    @State private var hasPermission = false
    @State private var selectedMode: InputMode = .telex

    private let timer = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    var body: some View {
        VStack(spacing: 0) {
            // Content
            contentView
                .frame(height: 340)

            Divider()

            // Footer
            HStack {
                StepIndicator(current: currentStep, total: totalSteps)
                Spacer()
                actionButton
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 14)
        }
        .frame(width: 480)
        .onAppear { hasPermission = AXIsProcessTrusted() }
        .onReceive(timer) { _ in hasPermission = AXIsProcessTrusted() }
    }

    // MARK: - Content

    private var totalSteps: Int { hasPermission ? 2 : 3 }

    @ViewBuilder
    private var contentView: some View {
        if hasPermission {
            // Đã có quyền: Success → Setup
            if currentStep == 0 {
                SuccessView()
            } else {
                SetupView(selectedMode: $selectedMode)
            }
        } else {
            // Chưa có quyền: Welcome → Permission → Setup
            switch currentStep {
            case 0: WelcomeView()
            case 1: PermissionView()
            default: SetupView(selectedMode: $selectedMode)
            }
        }
    }

    @ViewBuilder
    private var actionButton: some View {
        if hasPermission {
            // Flow có quyền
            if currentStep == 0 {
                Button("Tiếp tục") { currentStep = 1 }
                    .buttonStyle(.borderedProminent)
            } else {
                Button("Hoàn tất") { finish() }
                    .buttonStyle(.borderedProminent)
            }
        } else {
            // Flow chưa có quyền
            switch currentStep {
            case 0:
                Button("Tiếp tục") { currentStep = 1 }
                    .buttonStyle(.borderedProminent)
            case 1:
                Button("Mở Cài đặt") { openSettings() }
                    .buttonStyle(.borderedProminent)
            default:
                Button("Hoàn tất") { finish() }
                    .buttonStyle(.borderedProminent)
            }
        }
    }

    // MARK: - Actions

    private func openSettings() {
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!)
    }

    private func finish() {
        UserDefaults.standard.set(selectedMode.rawValue, forKey: SettingsKey.method)
        UserDefaults.standard.set(true, forKey: SettingsKey.hasCompletedOnboarding)
        NotificationCenter.default.post(name: .onboardingCompleted, object: nil)
        NSApp.keyWindow?.close()
    }
}

// MARK: - Step Indicator

private struct StepIndicator: View {
    let current: Int
    let total: Int

    var body: some View {
        HStack(spacing: 6) {
            ForEach(0..<total, id: \.self) { i in
                Circle()
                    .fill(i == current ? Color.accentColor : Color.secondary.opacity(0.3))
                    .frame(width: 6, height: 6)
            }
        }
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

private struct SuccessView: View {
    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 48))
                .foregroundStyle(.green)
            Text("Đã cấp quyền thành công")
                .font(.system(size: 22, weight: .bold))
            Text("\(AppMetadata.name) đã sẵn sàng hoạt động.")
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(.horizontal, 40)
    }
}

private struct PermissionView: View {
    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "hand.raised.fill")
                .font(.system(size: 40))
                .foregroundStyle(.orange)
            Text("Cấp quyền Accessibility")
                .font(.system(size: 22, weight: .bold))
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
