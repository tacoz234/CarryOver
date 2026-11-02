import SwiftUI

struct ContentView: View {
    @Bindable var model: SyncModel

    var body: some View {
        NavigationStack {
            Group {
                if model.group == nil {
                    JoinView(model: model)
                } else {
                    SyncView(model: model)
                }
            }
            .navigationTitle("CopyBlip")
        }
        .sheet(isPresented: $model.showingSetup, onDismiss: model.setupClosed) {
            SetupGuide()
        }
        .onOpenURL { model.open($0) }
        .alert(
            model.group == nil ? "Join this CopyBlip group?" : "Switch to this CopyBlip group?",
            isPresented: Binding(get: { model.pendingInvite != nil }, set: { if !$0 { model.pendingInvite = nil } }),
            presenting: model.pendingInvite
        ) { _ in
            Button(model.group == nil ? "Join" : "Switch") { model.acceptInvite() }
            Button("Cancel", role: .cancel) {}
        } message: { invite in
            let replaces = model.group == nil ? "" : " This replaces the group you're in now."
            let relay = invite.relay.map { " It also uses the relay at \($0.base.host() ?? "")." } ?? ""
            Text("Only join groups made on your own computers.\(replaces)\(relay)")
        }
    }
}

private struct JoinView: View {
    let model: SyncModel
    @State private var code = ""
    @State private var invalid = false

    var body: some View {
        Form {
            Section {
                TextField("Invite link or pairing code", text: $code, axis: .vertical)
                    .font(.body.monospaced())
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                Button("Join") { invalid = !model.join(code: code) }
                    .disabled(code.isEmpty)
            } footer: {
                Text("On your computer, run `copyblip invite`, then scan the QR code with the Camera app. Or paste the link it prints here.")
            }
            if invalid {
                Text("That isn't a valid invite link or pairing code.")
                    .foregroundStyle(.red)
            }
        }
    }
}

private struct SyncView: View {
    let model: SyncModel

    var body: some View {
        List {
            Section {
                StatusRow(status: model.status)
                    .padding(.vertical, 8)
            } footer: {
                Text("Just open CopyBlip. It sends what you copied on this iPhone, or gets what you last copied on your computer. Then paste as usual.")
            }
            Section("If something didn't sync") {
                // The system paste button never shows the paste prompt, but iOS
                // doesn't allow changing its label, so the row explains it.
                HStack {
                    Text("Send clipboard")
                    Spacer()
                    PasteButton(supportedContentTypes: [.plainText, .png, .image]) { providers in
                        model.send(pasted: providers)
                    }
                    .labelStyle(.iconOnly)
                }
                Button("Get Latest from Computer", systemImage: "arrow.down.doc") { model.fetch() }
            }
            Section {
                NavigationLink {
                    AwayFromHomeView(model: model)
                } label: {
                    Label("Away From Home", systemImage: "globe")
                }
                Button("Setup Guide", systemImage: "questionmark.circle") { model.showingSetup = true }
                Button("Leave Group", role: .destructive) { model.leave() }
            }
        }
    }
}

private struct SetupGuide: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Text("CopyBlip needs two permissions so opening it is all it takes.")
                }
                Section {
                    Step(number: 1, title: "Allow pasting without asking") {
                        Text("Without this, iOS asks \"Allow Paste?\" every time CopyBlip sends what you copied.")
                        Text("Open Settings below, tap **Paste from Other Apps** and choose **Allow**.")
                        Text("If the option isn't there yet, copy some text, open CopyBlip and tap **Allow Paste** once. Then come back here.")
                            .foregroundStyle(.secondary)
                    }
                    Button("Open CopyBlip Settings", systemImage: "gear") {
                        openURL(URL(string: UIApplication.openSettingsURLString)!)
                    }
                }
                Section {
                    Step(number: 2, title: "Allow Local Network") {
                        Text("This lets CopyBlip find your computer on your Wi-Fi. Choose **Allow** if iOS asks.")
                        Text("If you chose Don't Allow, turn on **Local Network** on the same Settings page.")
                            .foregroundStyle(.secondary)
                    }
                }
                Section {
                    Step(number: 3, title: "Use it") {
                        Text("**From iPhone:** copy something, open CopyBlip, then paste on your computer.")
                        Text("**To iPhone:** copy something on your computer, open CopyBlip, then paste on your iPhone.")
                    }
                }
            }
            .navigationTitle("Set Up CopyBlip")
            .toolbar {
                Button("Done") { dismiss() }
            }
        }
    }
}

private struct Step<Content: View>: View {
    let number: Int
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label {
                Text(title).font(.headline)
            } icon: {
                Image(systemName: "\(number).circle.fill").foregroundStyle(.tint)
            }
            content.font(.callout)
        }
        .padding(.vertical, 4)
    }
}

private struct StatusRow: View {
    let status: SyncModel.Status

    var body: some View {
        switch status {
        case .idle:
            Label("Ready", systemImage: "checkmark.circle")
        case let .working(message):
            HStack {
                ProgressView()
                Text(message)
            }
        case let .sent(preview, names):
            VStack(alignment: .leading, spacing: 6) {
                Label("Sent to \(names.joined(separator: ", "))", systemImage: "arrow.up.circle.fill")
                    .foregroundStyle(.green)
                    .font(.headline)
                PreviewView(preview: preview)
            }
        case let .received(preview, name):
            VStack(alignment: .leading, spacing: 6) {
                Label("Copied from \(name), ready to paste", systemImage: "arrow.down.circle.fill")
                    .foregroundStyle(.blue)
                    .font(.headline)
                PreviewView(preview: preview)
            }
        case .upToDate:
            Label("Up to date", systemImage: "checkmark.circle.fill")
        case let .failed(message):
            Label(message, systemImage: "exclamationmark.triangle.fill")
                .foregroundStyle(.orange)
        }
    }
}

private struct PreviewView: View {
    let preview: SyncModel.Preview

    var body: some View {
        switch preview {
        case let .text(text):
            Text(text).font(.callout).foregroundStyle(.secondary).lineLimit(3)
        case let .image(image):
            Image(uiImage: image)
                .resizable()
                .scaledToFit()
                .frame(maxHeight: 160)
                .clipShape(RoundedRectangle(cornerRadius: 8))
        }
    }
}

private struct AwayFromHomeView: View {
    let model: SyncModel
    @State private var relayText = ""
    @State private var relayInvalid = false
    @State private var newAddress = ""
    @State private var addressInvalid = false

    var body: some View {
        Form {
            Section {
                TextField("https://copyblip-relay.example.workers.dev", text: $relayText)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .onSubmit { relayInvalid = !model.setRelay(relayText) }
                if relayInvalid {
                    Text("That isn't a relay address. It should start with https://.")
                        .foregroundStyle(.red)
                }
            } header: {
                Text("Relay")
            } footer: {
                Text("Works on any network. Use the same relay address as your computers. Leave empty to turn it off.")
            }

            Section {
                ForEach(model.manualAddresses, id: \.self) { address in
                    Text(address.description).font(.body.monospaced())
                }
                .onDelete { model.removeAddresses(at: $0) }
                HStack {
                    TextField("macmini or 100.64.0.1", text: $newAddress)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .onSubmit(add)
                    Button("Add", action: add).disabled(newAddress.isEmpty)
                }
                if addressInvalid {
                    Text("That isn't a computer name or address.").foregroundStyle(.red)
                }
            } header: {
                Text("Computer Addresses")
            } footer: {
                Text("For Tailscale: with Tailscale on, add each computer's Tailscale name or 100.x address.")
            }
        }
        .navigationTitle("Away From Home")
        .onAppear { relayText = model.relay?.base.absoluteString ?? "" }
        .onDisappear { _ = model.setRelay(relayText) }
    }

    private func add() {
        addressInvalid = !model.addAddress(newAddress)
        if !addressInvalid { newAddress = "" }
    }
}
