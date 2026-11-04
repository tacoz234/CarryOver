import CopyBlipKit
import SwiftUI

/// A shared visual rhythm for welcome, pairing, and the first-run checklist.
struct OnboardingView: View {
    let model: SyncModel
    @Environment(\.openURL) private var openURL
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var step = 0
    @State private var joining = true
    @State private var code = ""
    @State private var error: String?
    @State private var scanning = false
    @State private var newGroup = false
    @State private var copied = false
    @State private var checkingConnection = false
    @State private var connectedNames: [String] = []
    @State private var connectionFailed = false
    @FocusState private var editingInvite: Bool

    private var complete: Bool { model.group != nil }
    private var page: Int { complete ? 2 : step }
    private var canContinue: Bool {
        complete || step == 0 || !joining || !code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        GeometryReader { geometry in
            ScrollView {
                VStack(spacing: 28) {
                    header
                    if complete {
                        completion
                    } else if step == 0 {
                        welcome
                    } else {
                        pairing
                    }
                    Spacer(minLength: 0)
                }
                .frame(maxWidth: 480)
                .padding(.horizontal, 24)
                .padding(.top, 16)
                .padding(.bottom, 24)
                .frame(minHeight: geometry.size.height, alignment: .top)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .background {
            ZStack(alignment: .top) {
                Color(.systemGroupedBackground)
                RadialGradient(colors: [Brand.accent.opacity(0.09), .clear], center: .top, startRadius: 20, endRadius: 420)
            }.ignoresSafeArea()
        }
        .safeAreaInset(edge: .bottom, spacing: 0) { footer }
        .toolbar(.hidden, for: .navigationBar)
        .sheet(isPresented: $scanning) {
            InviteScanner { value in
                code = value
                error = nil
                scanning = false
            }
        }
        .task(id: model.group) {
            if model.group != nil { await checkConnection() }
        }
        .onChange(of: code) { _, _ in error = nil }
        .onChange(of: joining) { _, _ in error = nil }
    }

    private var header: some View {
        HStack {
            HStack(spacing: 7) {
                Image(systemName: "clipboard.fill").foregroundStyle(Brand.accent)
                Text("Carryover").fontWeight(.semibold)
            }.font(.subheadline)
            Spacer()
            HStack(spacing: 5) {
                ForEach(0..<3) { index in
                    Capsule().fill(index <= page ? Brand.accent : Color.primary.opacity(0.1))
                        .frame(width: index == page ? 22 : 7, height: 5)
                }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Step \(page + 1) of 3")
        }
        .padding(.vertical, 8)
    }

    private var welcome: some View {
        VStack(spacing: 26) {
            deviceIllustration
                .padding(.top, 10)
            VStack(spacing: 12) {
                Text("A little less sending.\nA lot more pasting.")
                    .font(.system(.largeTitle, design: .rounded, weight: .bold))
                    .tracking(-0.8)
                    .fixedSize(horizontal: false, vertical: true)
                Text("Copy on your iPhone, Mac or PC.\nIt's ready on your other devices.")
                    .font(.body).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .multilineTextAlignment(.center)
            VStack(spacing: 0) {
                feature("One clipboard, all your devices", detail: "Text and images, over the same Wi-Fi.", icon: "laptopcomputer.and.iphone")
                Divider().padding(.leading, 62)
                feature("Pick up where you left off", detail: "Your recent clips, ready to copy again.", icon: "clock.arrow.circlepath")
            }
            .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 20))
        }
    }

    private var deviceIllustration: some View {
        ZStack {
            Circle().fill(Brand.accent.opacity(0.045)).frame(width: 220, height: 220)
            Circle().stroke(Brand.accent.opacity(0.09), lineWidth: 1).frame(width: 182, height: 182)
            HStack(spacing: 62) {
                Image(systemName: "desktopcomputer").font(.system(size: 61, weight: .light))
                    .rotationEffect(.degrees(-8)).offset(y: 8)
                Image(systemName: "iphone").font(.system(size: 65, weight: .light))
                    .rotationEffect(.degrees(9)).offset(y: -9)
            }.foregroundStyle(.primary.opacity(0.7))
            RoundedRectangle(cornerRadius: 25, style: .continuous)
                .fill(Brand.gradient)
                .frame(width: 94, height: 94)
                .shadow(color: Brand.accent.opacity(0.22), radius: 18, y: 12)
                .overlay {
                    Image(systemName: "clipboard.fill").font(.system(size: 43, weight: .medium)).foregroundStyle(.white)
                }
                .rotationEffect(.degrees(-5))
            Image(systemName: "arrow.right.arrow.left")
                .font(.system(size: 15, weight: .bold)).foregroundStyle(Brand.accent)
                .padding(11).background(Color(.secondarySystemGroupedBackground), in: Circle())
                .shadow(color: .black.opacity(0.04), radius: 8, y: 4)
                .offset(x: 47, y: 48)
        }
        .frame(height: 220)
        .accessibilityHidden(true)
    }

    private var pairing: some View {
        VStack(alignment: .leading, spacing: 20) {
            heading("Connect your devices", subtitle: "Already have Carryover? Bring this iPhone into your group.")
            VStack(spacing: 12) {
                choice("I already use Carryover", detail: "Join your other devices.", icon: "laptopcomputer.and.iphone", selected: joining) { joining = true }
                choice("This is my first device", detail: "Start a group and add devices later.", icon: "plus", selected: !joining) { joining = false }
            }
            if joining {
                VStack(alignment: .leading, spacing: 16) {
                    Text("YOUR INVITE").font(.caption.weight(.semibold)).tracking(1).foregroundStyle(.secondary)
                    Text("On your other device, open Carryover and choose Add a Device.")
                        .font(.callout).foregroundStyle(.secondary)
                    Button {
                        editingInvite = false
                        scanning = true
                    } label: {
                        Label("Scan QR Code", systemImage: "qrcode.viewfinder")
                            .font(.body.weight(.semibold)).frame(maxWidth: .infinity).padding(.vertical, 14)
                    }
                    .buttonStyle(.plain).foregroundStyle(Brand.accent)
                    .background(Brand.accent.opacity(0.08), in: RoundedRectangle(cornerRadius: 13))
                    HStack(spacing: 12) {
                        Rectangle().frame(height: 1)
                        Text("or paste a link").font(.caption).fixedSize()
                        Rectangle().frame(height: 1)
                    }.foregroundStyle(.secondary.opacity(0.5))
                    TextField("Invite link or pairing code", text: $code, axis: .vertical)
                        .font(.callout.monospaced()).lineLimit(2...4)
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                        .focused($editingInvite)
                        .padding(14).background(Color(.tertiarySystemFill), in: RoundedRectangle(cornerRadius: 12))
                        .accessibilityIdentifier("setup-invite")
                }
                .padding(20).background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 20))
            }
            if let error {
                Label(error, systemImage: "exclamationmark.circle.fill")
                    .font(.callout).foregroundStyle(.red)
                    .padding(14).frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.red.opacity(0.07), in: RoundedRectangle(cornerRadius: 14))
                    .accessibilityIdentifier("setup-error")
            }
            Label("Only join groups from your own devices. Anyone with the invite can read what you copy.", systemImage: "lock.shield")
                .font(.footnote).foregroundStyle(.secondary)
        }
    }

    private func heading(_ title: String, subtitle: String) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.system(.largeTitle, design: .rounded, weight: .bold)).tracking(-0.6)
            Text(subtitle).font(.body).foregroundStyle(.secondary)
        }.fixedSize(horizontal: false, vertical: true)
    }

    private func feature(_ title: String, detail: String, icon: String) -> some View {
        HStack(spacing: 14) {
            Image(systemName: icon).font(.system(size: 20, weight: .medium))
                .foregroundStyle(Brand.accent).frame(width: 28)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.subheadline.weight(.semibold))
                Text(detail).font(.footnote).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }.padding(18).frame(maxWidth: .infinity, alignment: .leading)
    }

    private func choice(_ title: String, detail: String, icon: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 14) {
                Image(systemName: icon).font(.system(size: 21, weight: .medium))
                    .foregroundStyle(Brand.accent).frame(width: 30)
                VStack(alignment: .leading, spacing: 5) {
                    Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(.primary)
                    Text(detail).font(.footnote).foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .font(.title3).foregroundStyle(selected ? Brand.accent : Color.secondary.opacity(0.4))
            }
            .padding(18).background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 17))
            .overlay(RoundedRectangle(cornerRadius: 17).stroke(selected ? Brand.accent : .clear, lineWidth: 1.5))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var footer: some View {
        VStack(spacing: 12) {
            if step == 0 && !complete {
                Text("A few taps, and you're connected.").font(.footnote).foregroundStyle(.secondary)
            }
            HStack(spacing: 12) {
                if step > 0 && !complete {
                    Button {
                        editingInvite = false
                        changeStep(0)
                    } label: {
                        Image(systemName: "chevron.left").font(.body.weight(.semibold))
                            .frame(width: 52, height: 54)
                    }
                    .buttonStyle(.plain).foregroundStyle(.primary)
                    .background(Color(.quaternarySystemFill), in: RoundedRectangle(cornerRadius: 16))
                    .accessibilityLabel("Back")
                }
                Button {
                    editingInvite = false
                    if complete { model.finishOnboarding() }
                    else if step == 0 { changeStep(1) }
                    else { setUp() }
                } label: {
                    HStack {
                        Spacer()
                        Text(complete ? "Open Carryover" : step == 0 ? "Get Started" : joining ? "Join Group" : "Create Group")
                        Image(systemName: "arrow.right").font(.subheadline.weight(.semibold))
                        Spacer()
                    }
                    .font(.body.weight(.semibold)).padding(.vertical, 17)
                    .foregroundStyle(.white)
                    .background(Color(red: 0.816, green: 0.243, blue: 0.149), in: RoundedRectangle(cornerRadius: 16))
                    .opacity(canContinue ? 1 : 0.4)
                }
                .buttonStyle(.plain).disabled(!canContinue)
            }
        }
        .frame(maxWidth: 480)
        .padding(.horizontal, 24).padding(.top, 16).padding(.bottom, 12)
        .frame(maxWidth: .infinity)
        .background(.regularMaterial)
    }

    private func changeStep(_ value: Int) {
        withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.2)) { step = value; error = nil }
    }

    private func setUp() {
        if joining && Invite(code) == nil {
            error = "That isn't a Carryover invite. Copy the link again or scan the QR code on your other device."
            return
        }
        let saved = joining ? model.join(code: code) : model.createGroup()
        if saved { newGroup = !joining } else {
            error = "Couldn't securely save your group on this iPhone. Try again."
        }
    }

    @MainActor private func checkConnection() async {
        checkingConnection = true
        connectionFailed = false
        do {
            connectedNames = try await model.verifyGroupConnection()
        } catch {
            if !Task.isCancelled { connectionFailed = true }
        }
        checkingConnection = false
    }

    private var connectionStatus: some View {
        VStack(alignment: .leading, spacing: 12) {
            if checkingConnection {
                HStack { ProgressView(); Text("Checking your devices…") }
            } else if !connectedNames.isEmpty {
                Label("Connected to " + connectedNames.joined(separator: ", "), systemImage: "checkmark.circle.fill")
                    .foregroundStyle(Brand.accent)
            } else if !newGroup {
                Label("No devices reached yet", systemImage: "wifi.exclamationmark").fontWeight(.semibold)
                Text("Keep Carryover open on your other device, use the same Wi-Fi, and allow Local Network in Settings. If you used an older invite, scan a fresh QR code from the updated Mac app.")
                    .font(.callout).foregroundStyle(.secondary)
                Button("Try Again") { Task { await checkConnection() } }.disabled(checkingConnection)
            } else {
                Label("Ready for your next device", systemImage: "plus.circle")
            }
        }
        .font(.subheadline).padding(18).frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 20))
    }

    private var completion: some View {
        VStack(spacing: 24) {
            Image(systemName: "checkmark").font(.system(size: 32, weight: .semibold))
                .foregroundStyle(Brand.accent).frame(width: 80, height: 80)
                .background(Brand.accent.opacity(0.1), in: RoundedRectangle(cornerRadius: 26))
            VStack(spacing: 10) {
                Text(connectionFailed && !newGroup ? "Group saved" : "You're all set").font(.system(.largeTitle, design: .rounded, weight: .bold))
                Text(connectionFailed && !newGroup ? "Your invite was saved, but no device has answered yet." : "Your group is ready. Here's how to make\nsharing feel effortless.")
                    .foregroundStyle(.secondary).multilineTextAlignment(.center)
            }
            connectionStatus
            if let link = model.inviteLink, newGroup {
                VStack(spacing: 14) {
                    Text("Add your next device").font(.headline)
                    if let qr = QRCode.image(for: link) {
                        Image(uiImage: qr).interpolation(.none).resizable().scaledToFit()
                            .frame(maxWidth: 170).padding(12).background(.white, in: RoundedRectangle(cornerRadius: 12))
                            .accessibilityLabel("QR code to join your Carryover group")
                    }
                    Text("Scan this in Carryover on another iPhone, or paste the invite into setup on your computer.")
                        .font(.callout).foregroundStyle(.secondary).multilineTextAlignment(.center)
                    Button(copied ? "Copied" : "Copy Invite Link", systemImage: copied ? "checkmark" : "link") {
                        model.copyWithoutSending(link); copied = true
                    }.buttonStyle(.bordered).tint(Brand.accent)
                }
                .padding(20).frame(maxWidth: .infinity)
                .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 20))
            }
            VStack(spacing: 0) {
                feature("Allow Local Network", detail: "Choose Allow when asked to find devices on your Wi-Fi.", icon: "wifi")
                Divider().padding(.leading, 62)
                feature("Allow pasting", detail: "For automatic sharing, set Paste from Other Apps to Allow in Settings after your first paste request.", icon: "doc.on.clipboard")
                Divider().padding(.leading, 62)
                Button("Open Carryover Settings", systemImage: "arrow.up.forward.app") {
                    openURL(URL(string: UIApplication.openSettingsURLString)!)
                }.font(.subheadline.weight(.medium)).padding(18)
            }
            .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 20))
            Text("Copy something, open Carryover, then paste.\niPhone syncs while the app is open.")
                .font(.footnote).foregroundStyle(.secondary).multilineTextAlignment(.center)
        }
    }
}
