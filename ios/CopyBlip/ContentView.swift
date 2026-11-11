import CoreImage.CIFilterBuiltins
import SwiftUI

struct ContentView: View {
    @Bindable var model: SyncModel

    var body: some View {
        NavigationStack {
            Group {
                if model.needsOnboarding {
                    OnboardingView(model: model)
                } else {
                    SyncView(model: model)
                }
            }
            .navigationTitle("Carryover")
        }
        .tint(Brand.accent)
        .sheet(isPresented: $model.showingSetup, onDismiss: model.setupClosed) {
            SetupGuide(model: model)
        }
        .onOpenURL { model.open($0) }
        .alert(
            model.group == nil ? "Join this Carryover group?" : "Switch to this Carryover group?",
            isPresented: Binding(get: { model.pendingInvite != nil }, set: { if !$0 { model.pendingInvite = nil } }),
            presenting: model.pendingInvite
        ) { _ in
            Button(model.group == nil ? "Join" : "Switch") { model.acceptInvite() }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            let replaces = model.group == nil ? "" : " This replaces the group you're in now."
            Text("Only join groups made on your own computers.\(replaces)")
        }
    }
}

/// Public pages also used by the App Store listing.
enum AppLinks {
    static let privacy = URL(string: "https://tacoz234.github.io/CarryOver/privacy.html")!
    static let support = URL(string: "https://tacoz234.github.io/CarryOver/support.html")!
}

/// Carryover's coral, matching the computers' window.
enum Brand {
    /// #E0482D, readable both on white and in dark mode.
    static let accent = Color(red: 0.878, green: 0.282, blue: 0.176)
    /// #FF9158 to #F0533C.
    static var gradient: LinearGradient {
        LinearGradient(
            colors: [Color(red: 1.0, green: 0.569, blue: 0.345), Color(red: 0.941, green: 0.325, blue: 0.235)],
            startPoint: .top,
            endPoint: .bottom
        )
    }
}

/// How a device's reported name is shown, and which icon it gets.
private enum DeviceLook {
    /// coles-Mac-Mini.local becomes coles Mac Mini.
    static func name(_ name: String) -> String {
        var name = name
        if name.hasSuffix(".local") { name.removeLast(".local".count) }
        return name.replacingOccurrences(of: "-", with: " ")
    }

    /// Phones can't stay connected in the background, so they're never
    /// shown as offline.
    static func isPhone(_ name: String) -> Bool {
        let name = name.lowercased()
        return name.contains("iphone") || name.contains("ipad")
    }

    static func symbol(_ name: String) -> String {
        let name = name.lowercased()
        if name.contains("iphone") { return "iphone" }
        if name.contains("ipad") { return "ipad" }
        if name.contains("macbook") || name.contains("laptop") { return "laptopcomputer" }
        if name.contains("mini") { return "macmini" }
        if name.contains("mac") { return "desktopcomputer" }
        return "pc"
    }
}

private struct SyncView: View {
    let model: SyncModel
    @State private var showingSettings = false
    @State private var showingAddDevice = false
    @State private var copiedID: String?

    @Environment(\.horizontalSizeClass) private var sizeClass

    /// iPhones fit four tiles across. Wider screens, like iPads, keep tiles
    /// at a fixed size and fit more across instead of stretching them.
    private var deviceColumns: [GridItem] {
        sizeClass == .regular
            ? [GridItem(.adaptive(minimum: 112, maximum: 128), spacing: 24, alignment: .top)]
            : Array(repeating: GridItem(.flexible(), spacing: 12, alignment: .top), count: 4)
    }

    private var recentColumns: [GridItem] {
        sizeClass == .regular
            ? [GridItem(.adaptive(minimum: 200, maximum: 240), spacing: 24, alignment: .top)]
            : Array(repeating: GridItem(.flexible(), spacing: 16, alignment: .top), count: 2)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                StatusCard(status: model.status)

                LazyVGrid(columns: deviceColumns, spacing: 20) {
                    ForEach(model.devices) { device in
                        DeviceTile(device: device, badge: badge(for: device.name), online: online(device.name))
                    }
                    ActionTile(title: "Add a Device", symbol: "plus") { showingAddDevice = true }
                    ActionTile(title: "Get Latest", symbol: "arrow.down") { model.fetch() }
                }
                if model.devices.isEmpty {
                    Text("Your computers show up here after the first sync.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }

                SendCard(model: model)

                Divider()

                recent
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button("Settings", systemImage: "gearshape") { showingSettings = true }
            }
        }
        .sheet(isPresented: $showingSettings) {
            SettingsSheet(model: model)
        }
        .sheet(isPresented: $showingAddDevice) {
            AddDeviceSheet(model: model)
        }
        .task(id: copiedID) {
            guard copiedID != nil else { return }
            try? await Task.sleep(for: .seconds(1.5))
            copiedID = nil
        }
    }

    @ViewBuilder private var recent: some View {
        HStack {
            Text("Recent").font(.title.bold())
            Spacer()
            if !model.history.isEmpty {
                Menu {
                    Button("Clear Recent", systemImage: "trash", role: .destructive) { model.clearHistory() }
                } label: {
                    Image(systemName: "ellipsis")
                        .font(.title3)
                        .frame(width: 44, height: 44)
                }
            }
        }
        if model.history.isEmpty {
            Text("What you send and receive shows up here. Tap something to copy it again.")
                .font(.callout)
                .foregroundStyle(.secondary)
        } else {
            LazyVGrid(columns: recentColumns, spacing: 24) {
                ForEach(model.history) { item in
                    Button {
                        model.copy(item)
                        copiedID = item.id
                    } label: {
                        RecentCard(item: item, thumbnail: model.thumbnail(for: item), copied: copiedID == item.id)
                    }
                    .buttonStyle(.plain)
                    .contextMenu {
                        Button("Copy", systemImage: "doc.on.doc") {
                            model.copy(item)
                            copiedID = item.id
                        }
                        Button("Delete", systemImage: "trash", role: .destructive) { model.delete(item) }
                    }
                }
            }
        }
    }

    /// Nil when unknown: before the first sync, and for phones.
    private func online(_ name: String) -> Bool? {
        guard let online = model.online, !DeviceLook.isPhone(name) else { return nil }
        return online.contains(name)
    }

    /// Marks the devices the last sync went to or came from.
    private func badge(for name: String) -> DeviceTile.Badge? {
        switch model.status {
        case let .sent(_, names) where names.contains(name): return DeviceTile.Badge.sent
        case let .received(_, from) where from == name: return DeviceTile.Badge.received
        default: return nil
        }
    }
}

private struct DeviceTile: View {
    enum Badge { case sent, received }

    let device: SyncModel.Device
    let badge: Badge?
    let online: Bool?

    var body: some View {
        VStack(spacing: 8) {
            Circle()
                .fill(Brand.gradient)
                .opacity(online == false ? 0.45 : 1)
                .overlay {
                    Image(systemName: DeviceLook.symbol(device.name))
                        .font(.system(size: 28, weight: .medium))
                        .foregroundStyle(.white)
                }
                .overlay(alignment: .bottomTrailing) {
                    if let badge {
                        Image(systemName: badge == .sent ? "checkmark.circle.fill" : "arrow.down.circle.fill")
                            .font(.title3)
                            .foregroundStyle(Color.white, badge == .sent ? Color.green : Color.blue)
                            .background(Circle().fill(Color(.systemBackground)).padding(-2))
                            .transition(.scale.combined(with: .opacity))
                    }
                }
                .overlay(alignment: .topTrailing) {
                    if let online {
                        Circle()
                            .fill(online ? Color.green : Color.gray)
                            .frame(width: 14, height: 14)
                            .overlay(Circle().stroke(Color(.systemBackground), lineWidth: 2.5))
                            .padding(3)
                    }
                }
                .aspectRatio(1, contentMode: .fit)
                .animation(.spring, value: badge)
                .animation(.default, value: online)
            Text(DeviceLook.name(device.name))
                .font(.footnote)
                .multilineTextAlignment(.center)
                .lineLimit(2)
        }
        .accessibilityElement(children: .combine)
        .accessibilityValue(online.map { $0 ? "Online" : "Offline" } ?? "")
    }
}

private struct ActionTile: View {
    let title: String
    let symbol: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 8) {
                Circle()
                    .fill(Color(.secondarySystemFill))
                    .overlay {
                        Image(systemName: symbol)
                            .font(.system(size: 26, weight: .semibold))
                            .foregroundStyle(Brand.accent)
                    }
                    .aspectRatio(1, contentMode: .fit)
                Text(title)
                    .font(.footnote)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                    .foregroundStyle(.primary)
            }
        }
        .buttonStyle(.plain)
    }
}

/// The system paste button never shows the paste prompt, but iOS doesn't
/// allow changing its label, so the card explains it.
private struct SendCard: View {
    let model: SyncModel

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Send Clipboard").font(.headline)
                Text("If what you copied didn't send when you opened Carryover.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            PasteButton(supportedContentTypes: [.plainText, .png, .image]) { providers in
                model.send(pasted: providers)
            }
            .labelStyle(.iconOnly)
            .buttonBorderShape(.circle)
            .tint(Brand.accent)
        }
        .padding(14)
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 16))
    }
}

private struct RecentCard: View {
    let item: SyncModel.HistoryItem
    let thumbnail: UIImage?
    let copied: Bool

    var body: some View {
        VStack(spacing: 6) {
            preview
                .frame(maxWidth: .infinity)
                .frame(height: 150)
                .overlay {
                    if copied {
                        Label("Copied", systemImage: "checkmark")
                            .font(.subheadline.bold())
                            .padding(.horizontal, 12)
                            .padding(.vertical, 6)
                            .background(.ultraThinMaterial, in: Capsule())
                            .transition(.opacity)
                    }
                }
                .animation(.easeOut(duration: 0.2), value: copied)
                .overlay(alignment: .bottom) {
                    Circle()
                        .fill(Brand.gradient)
                        .frame(width: 36, height: 36)
                        .overlay {
                            Image(systemName: avatarSymbol)
                                .font(.system(size: 15, weight: .semibold))
                                .foregroundStyle(.white)
                        }
                        .overlay(Circle().stroke(Color(.systemBackground), lineWidth: 3))
                        .offset(y: 18)
                }
                .padding(.bottom, 18)
            Text(title)
                .font(.subheadline)
                .lineLimit(2)
                .multilineTextAlignment(.center)
            Text(caption)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .contentShape(Rectangle())
    }

    @ViewBuilder private var preview: some View {
        if let thumbnail {
            Image(uiImage: thumbnail)
                .resizable()
                .scaledToFit()
                .clipShape(RoundedRectangle(cornerRadius: 10))
                .shadow(color: .black.opacity(0.12), radius: 4, y: 1)
        } else {
            RoundedRectangle(cornerRadius: 10)
                .fill(Color(.secondarySystemBackground))
                .overlay(alignment: .topLeading) {
                    Text(item.text ?? "An image")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(7)
                        .padding(10)
                }
        }
    }

    private var avatarSymbol: String {
        if item.direction == .sent, item.peers.count != 1 { return "arrow.up" }
        return item.peers.first.map(DeviceLook.symbol) ?? "questionmark"
    }

    private var title: String {
        guard let text = item.text else { return "Image" }
        let line = text.split(whereSeparator: \.isNewline).first.map(String.init) ?? text
        return line.trimmingCharacters(in: .whitespaces)
    }

    private var caption: String {
        let names = item.peers.map(DeviceLook.name)
        let who = switch item.direction {
        case .received: "From \(names.first ?? "a device")"
        case .sent: names.count == 1 ? "Sent to \(names[0])" : "Sent to \(names.count) devices"
        }
        return "\(who) · \(item.date.formatted(.relative(presentation: .named)))"
    }
}

private struct SettingsSheet: View {
    let model: SyncModel
    @Environment(\.dismiss) private var dismiss
    @State private var confirmingLeave = false

    var body: some View {
        NavigationStack {
            List {
                Section {
                    NavigationLink {
                        SetupSteps(model: model)
                    } label: {
                        Label("Setup Guide", systemImage: "questionmark.circle")
                    }
                }
                if !model.devices.isEmpty {
                    Section {
                        ForEach(model.devices) { device in
                            Label(DeviceLook.name(device.name), systemImage: DeviceLook.symbol(device.name))
                        }
                        .onDelete { model.forgetDevices(at: $0) }
                    } header: {
                        Text("Devices")
                    } footer: {
                        Text("Swipe to remove a device you don't use anymore. It comes back if it syncs again.")
                    }
                }
                Section("Help & Privacy") {
                    Link(destination: AppLinks.support) {
                        Label("Support", systemImage: "questionmark.circle")
                    }
                    Link(destination: AppLinks.privacy) {
                        Label("Privacy Policy", systemImage: "hand.raised")
                    }
                }
                Section {
                    Button("Leave Group", role: .destructive) { confirmingLeave = true }
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                Button("Done") { dismiss() }
            }
            .confirmationDialog("Leave this group?", isPresented: $confirmingLeave, titleVisibility: .visible) {
                Button("Leave Group", role: .destructive) {
                    dismiss()
                    model.leave()
                }
            } message: {
                Text("This iPhone stops syncing and forgets its devices and recent items until you join again.")
            }
        }
    }
}

private struct AddDeviceSheet: View {
    let model: SyncModel
    @Environment(\.dismiss) private var dismiss
    @State private var showingLink = false
    @State private var copied = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 20) {
                    if let link = model.inviteLink {
                        Text("On another iPhone, open Carryover setup and choose Scan QR Code. You can also use the Camera app.")
                            .multilineTextAlignment(.center)
                        if let qr = QRCode.image(for: link) {
                            Image(uiImage: qr)
                                .interpolation(.none)
                                .resizable()
                                .scaledToFit()
                                .frame(maxWidth: 240)
                                .padding(16)
                                .background(.white, in: RoundedRectangle(cornerRadius: 16))
                        }
                        Text("On a computer, open Carryover's setup, choose I already use Carryover, and paste the invite link.")
                            .multilineTextAlignment(.center)
                        Button(showingLink ? "Hide Link" : "Show Link") { showingLink.toggle() }
                            .buttonStyle(.bordered)
                        if showingLink {
                            Text(link)
                                .font(.footnote.monospaced())
                                .textSelection(.enabled)
                                .padding(12)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 10))
                            Button(copied ? "Copied" : "Copy Link", systemImage: copied ? "checkmark" : "doc.on.doc") {
                                model.copyWithoutSending(link)
                                copied = true
                            }
                        }
                        Text("The invite holds your group's key. Anyone with it can read what you copy, so only use it with your own devices.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                    }
                }
                .padding(20)
            }
            .navigationTitle("Add a Device")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                Button("Done") { dismiss() }
            }
        }
    }
}

enum QRCode {
    static func image(for text: String) -> UIImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage?.transformed(by: CGAffineTransform(scaleX: 10, y: 10)),
              let image = CIContext().createCGImage(output, from: output.extent) else { return nil }
        return UIImage(cgImage: image)
    }
}

private struct SetupGuide: View {
    let model: SyncModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            SetupSteps(model: model)
                .toolbar {
                    Button("Done") { dismiss() }
                }
        }
    }
}

/// The setup steps, shown on first join and from Settings.
private struct SetupSteps: View {
    let model: SyncModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text("Set up once. Share with fewer interruptions.")
                    .font(.title2.bold())
                PermissionChecklist(model: model)
                Text("Copy something, open Carryover, then paste on your other device. Your devices need to be on the same Wi-Fi.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            .frame(maxWidth: 520).padding(20).frame(maxWidth: .infinity)
        }
        .background(Color(.systemGroupedBackground))
        .navigationTitle("Setup Guide")
    }
}

private struct StatusCard: View {
    let status: SyncModel.Status

    var body: some View {
        HStack(spacing: 10) {
            content
            Spacer(minLength: 0)
        }
        .font(.subheadline.weight(.medium))
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 16))
        .animation(.default, value: status)
    }

    @ViewBuilder private var content: some View {
        switch status {
        case .idle:
            Label("Ready", systemImage: "checkmark.circle")
        case let .working(message):
            ProgressView()
            Text(message)
        case let .sent(_, names):
            Label("Sent to \(names.map(DeviceLook.name).joined(separator: ", "))", systemImage: "arrow.up.circle.fill")
                .foregroundStyle(.green)
        case let .received(_, name):
            Label("Copied from \(DeviceLook.name(name)), ready to paste", systemImage: "arrow.down.circle.fill")
                .foregroundStyle(.blue)
        case .upToDate:
            Label("Up to date", systemImage: "checkmark.circle.fill")
        case let .failed(message):
            Label(message, systemImage: "exclamationmark.triangle.fill")
                .foregroundStyle(.orange)
        }
    }
}
