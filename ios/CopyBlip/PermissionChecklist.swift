import AVFoundation
import SwiftUI

/// Shared by first-run setup and Settings. Checkmarks describe verified facts,
/// never an inferred permanent permission that iOS doesn't expose.
struct PermissionChecklist: View {
    let model: SyncModel
    @Environment(\.openURL) private var openURL
    @Environment(\.scenePhase) private var scenePhase
    @State private var camera = AVCaptureDevice.authorizationStatus(for: .video)
    @State private var networkMessage: String?
    @State private var wasInBackground = false

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            card {
                statusRow("Paste access", icon: "doc.on.clipboard", verified: model.pasteCheck == .passed)
                Text("Tapping Allow Paste on the alert doesn't turn on permanent access.")
                    .font(.subheadline.weight(.medium))
                Text("1. Copy a short piece of text in Notes or another app.\n2. Return here and tap Test Paste Access. Choose Allow Paste if asked.\n3. Open Settings → Paste from Other Apps → Allow.")
                    .font(.callout).foregroundStyle(.secondary)
                Text("If Paste from Other Apps isn't listed yet, complete the test first, then reopen Settings.")
                    .font(.footnote).foregroundStyle(.secondary)
                Label(pasteMessage, systemImage: model.pasteCheck == .passed ? "checkmark.circle.fill" : "info.circle")
                    .font(.callout).foregroundStyle(model.pasteCheck == .passed ? Color.green : Color.secondary)
                    .accessibilityIdentifier("paste-check-status")
                HStack {
                    Button("Test Paste Access") { model.testPasteAccess() }
                        .buttonStyle(.bordered).accessibilityIdentifier("test-paste-access")
                    Spacer(minLength: 4)
                    settingsButton
                }
                Text("This test reads and discards the text. It doesn't send or save it. A passed test confirms this read worked; iOS doesn't tell Carryover whether Ask or Allow is selected in Settings.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            card {
                statusRow("Local Network", icon: "wifi", verified: model.localConnectionVerified)
                Text(model.localConnectionVerified
                    ? "A local device answered and authenticated. Local network access worked for that connection."
                    : "Allow Local Network in Settings so your devices can find each other. Keep Carryover open on a computer on the same Wi-Fi, then check the connection.")
                    .font(.callout).foregroundStyle(.secondary)
                if let networkMessage { Text(networkMessage).font(.callout).foregroundStyle(.secondary) }
                if model.verifyingGroupConnection {
                    ProgressView("Checking connection…").font(.callout)
                } else {
                    Button("Check Connection") {
                        Task {
                            networkMessage = nil
                            do { _ = try await model.verifyGroupConnection() }
                            catch { networkMessage = "No device answered. Check Wi-Fi, Local Network access, and that Carryover is running on your computer." }
                        }
                    }.buttonStyle(.bordered).disabled(model.group == nil)
                }
            }
            card {
                statusRow("Camera · optional", icon: "qrcode.viewfinder", verified: camera == .authorized)
                Text(cameraMessage).font(.callout).foregroundStyle(.secondary)
                if camera == .denied || camera == .restricted { settingsButton }
            }
        }
        .onAppear { model.permissionChecklistVisible = true }
        .onDisappear { model.permissionChecklistVisible = false }
        .task { camera = AVCaptureDevice.authorizationStatus(for: .video) }
        .onChange(of: scenePhase) { _, phase in
            if phase == .background { wasInBackground = true }
            if phase == .active {
                camera = AVCaptureDevice.authorizationStatus(for: .video)
                // Returning from Settings doesn't prove the user's choice.
                if wasInBackground {
                    model.resetPermissionChecks()
                    wasInBackground = false
                }
            }
        }
    }

    private var settingsButton: some View {
        Button("Open Settings", systemImage: "arrow.up.forward.app") {
            openURL(URL(string: UIApplication.openSettingsURLString)!)
        }.font(.subheadline.weight(.medium))
    }

    private var pasteMessage: String {
        switch model.pasteCheck {
        case .notChecked: "Not checked — run the test after copying text in another app."
        case .passed: "Paste test passed for this copy."
        case .denied: "Text couldn't be read. Set Paste from Other Apps to Allow, then try a fresh copy."
        case .empty: "Copy some text in Notes or another app first."
        case .ownCopy: "This copy came from Carryover. Copy fresh text in Notes to test access from another app."
        }
    }

    private var cameraMessage: String {
        switch camera {
        case .authorized: "Camera access is allowed for scanning invite QR codes."
        case .denied: "Camera access is off. Allow it in Settings to scan, or paste an invite link instead."
        case .restricted: "Camera access is restricted on this device. You can paste an invite link instead."
        case .notDetermined: "Not requested. Only needed if you choose Scan QR Code."
        @unknown default: "Camera status is unavailable. You can paste an invite link instead."
        }
    }

    private func statusRow(_ title: String, icon: String, verified: Bool) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon).foregroundStyle(Brand.accent)
            Text(title).font(.headline)
            Spacer(minLength: 4)
            Image(systemName: verified ? "checkmark.circle.fill" : "circle.dashed")
                .foregroundStyle(verified ? Color.green : Color.secondary)
                .accessibilityLabel(verified ? "Verified" : "Not verified")
        }
    }

    private func card<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 14, content: content)
            .padding(20).frame(maxWidth: .infinity, alignment: .leading)
            .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 20))
    }
}
