import CopyBlipKit
import Observation
import UIKit
import UniformTypeIdentifiers
import os

private let log = Logger(subsystem: "com.coledeterman.CopyBlip", category: "sync")

@MainActor
@Observable
final class SyncModel {
    enum Status: Equatable {
        case idle
        case working(String)
        case sent(Preview, to: [String])
        case received(Preview, from: String)
        case upToDate
        case failed(String)
    }

    enum Preview: Equatable {
        case text(String)
        case image(UIImage)
    }

    private(set) var status = Status.idle
    private(set) var group: GroupKey?
    var showingSetup = false
    private var running: Task<Void, Never>?
    /// System alerts, including the paste prompt, make the app inactive and then
    /// active again. Only a real open, from launch or the background, should sync.
    private var syncedSinceOpen = false

    private let defaults = UserDefaults.standard
    private let device: String

    init() {
        if let saved = defaults.string(forKey: "device") {
            device = saved
        } else {
            device = UUID().uuidString.lowercased()
            defaults.set(device, forKey: "device")
        }
        group = Keychain.load().flatMap { GroupKey(bytes: $0) as GroupKey? }
        manualAddresses = defaults.data(forKey: "manualAddresses")
            .flatMap { try? JSONDecoder().decode([Address].self, from: $0) } ?? []
        relay = defaults.string(forKey: "relay").flatMap(Relay.init(string:))
        #if DEBUG
        // Lets scripts pair a simulator without typing the code.
        if let code = ProcessInfo.processInfo.environment["COPYBLIP_PAIRING_CODE"] {
            // Scripts can't tap through the setup guide.
            defaults.set(true, forKey: "setupShown")
            if group != GroupKey(pairingCode: code) { _ = join(code: code) }
        }
        if let relay = ProcessInfo.processInfo.environment["COPYBLIP_RELAY"] {
            _ = setRelay(relay)
        }
        #endif
        if group != nil { showSetupOnce() }
    }

    private func showSetupOnce() {
        guard !defaults.bool(forKey: "setupShown") else { return }
        defaults.set(true, forKey: "setupShown")
        showingSetup = true
    }

    /// The pasteboard change count when we last sent or wrote it. Reading the
    /// count doesn't trigger the paste prompt; reading the contents does.
    private var handledChangeCount: Int? {
        get { defaults.object(forKey: "handledChangeCount") as? Int }
        set { defaults.set(newValue, forKey: "handledChangeCount") }
    }

    /// The clip this phone last sent or received, so opening the app again
    /// doesn't rewrite the same clip.
    private var lastClipID: String? {
        get { defaults.string(forKey: "lastClipID") }
        set { defaults.set(newValue, forKey: "lastClipID") }
    }

    /// Addresses that worked last time, tried before searching the network.
    private var addresses: [Address] {
        get { defaults.data(forKey: "addresses").flatMap { try? JSONDecoder().decode([Address].self, from: $0) } ?? [] }
        set { defaults.set(try? JSONEncoder().encode(newValue), forKey: "addresses") }
    }

    /// Typed-in addresses, such as Tailscale names, tried along with remembered ones.
    /// Stored, rather than read from defaults each time, so views update.
    private(set) var manualAddresses: [Address] = [] {
        didSet { defaults.set(try? JSONEncoder().encode(manualAddresses), forKey: "manualAddresses") }
    }

    private(set) var relay: Relay? {
        didSet { defaults.set(relay?.base.absoluteString, forKey: "relay") }
    }

    /// Returns false if `text` isn't a host or address.
    func addAddress(_ text: String) -> Bool {
        guard let address = Address(parsing: text) else { return false }
        if !manualAddresses.contains(address) { manualAddresses.append(address) }
        return true
    }

    func removeAddresses(at offsets: IndexSet) {
        manualAddresses.remove(atOffsets: offsets)
    }

    /// Clears the relay when `text` is empty. Returns false if it isn't a relay URL.
    func setRelay(_ text: String) -> Bool {
        if text.trimmingCharacters(in: .whitespaces).isEmpty {
            relay = nil
            return true
        }
        guard let parsed = Relay(string: text) else { return false }
        relay = parsed
        return true
    }

    /// An invite opened from a link, waiting for the user to confirm.
    var pendingInvite: Invite?

    func open(_ url: URL) {
        // The confirmation can't appear over the setup guide.
        showingSetup = false
        pendingInvite = Invite(url.absoluteString)
        if pendingInvite == nil {
            status = .failed("That CopyBlip link isn't valid. Make a new one with `copyblip invite`.")
        }
    }

    func acceptInvite() {
        guard let invite = pendingInvite else { return }
        pendingInvite = nil
        _ = join(code: invite.group.pairingCode, relay: invite.relay)
    }

    /// Accepts an invite link or a bare pairing code.
    func join(code: String) -> Bool {
        guard let invite = Invite(code) else { return false }
        return join(code: invite.group.pairingCode, relay: invite.relay)
    }

    private func join(code: String, relay: Relay?) -> Bool {
        guard let key = GroupKey(pairingCode: code), Keychain.save(key.bytes) else { return false }
        group = key
        if let relay { self.relay = relay }
        lastClipID = nil
        syncedSinceOpen = false
        addresses = []
        showSetupOnce()
        // Whatever is on the clipboard now predates joining, so don't send it.
        handledChangeCount = UIPasteboard.general.changeCount
        status = .idle
        return true
    }

    func leave() {
        Keychain.delete()
        group = nil
        addresses = []
        status = .idle
    }

    func appClosed() {
        syncedSinceOpen = false
    }

    /// Sends the clipboard if it changed since last time, otherwise fetches the
    /// latest clip from the other devices.
    func appOpened() {
        guard group != nil, !syncedSinceOpen, running == nil, !showingSetup else { return }
        syncedSinceOpen = true
        let board = UIPasteboard.general
        if board.changeCount != handledChangeCount, board.hasStrings || board.hasImages {
            log.info("clipboard changed since last sync, sending")
            // This read is what shows "Allow Paste" unless the user allowed it in Settings.
            // Left unhandled so a later open retries instead of fetching over
            // a copy the user hasn't shared yet.
            guard let content = Self.read(board) else {
                log.info("paste was refused")
                status = .failed("CopyBlip wasn't allowed to paste, so nothing was sent.")
                showingSetup = true
                return
            }
            send(content)
        } else {
            log.info("nothing new on this iPhone, fetching")
            fetch()
        }
    }

    /// Runs when the setup guide closes, so the first sync happens after it.
    func setupClosed() {
        showingSetup = false
        syncedSinceOpen = false
        appOpened()
    }

    /// Text wins when a copy has both, matching what a normal paste does.
    private static func read(_ board: UIPasteboard) -> Content? {
        if board.hasStrings {
            return board.string.map(Content.text)
        }
        if let png = board.data(forPasteboardType: UTType.png.identifier) {
            return .image(png)
        }
        return board.image?.pngData().map(Content.image)
    }

    func send(_ content: Content) {
        guard let client else { return }
        handledChangeCount = UIPasteboard.general.changeCount
        run("Sending…") {
            let (sent, addresses) = try await client.send(content)
            self.addresses = addresses
            self.lastClipID = sent.id
            return .sent(Self.preview(content), to: sent.to)
        }
    }

    /// For the paste button, which never shows the paste prompt.
    func send(pasted providers: [NSItemProvider]) {
        guard let provider = providers.first else { return }
        Task {
            if let text = await Self.text(from: provider) {
                send(.text(text))
            } else if let png = await Self.png(from: provider) {
                send(.image(png))
            }
        }
    }

    private static func text(from provider: NSItemProvider) async -> String? {
        guard provider.canLoadObject(ofClass: NSString.self) else { return nil }
        return await withCheckedContinuation { done in
            _ = provider.loadObject(ofClass: NSString.self) { text, _ in
                done.resume(returning: text as? String)
            }
        }
    }

    private static func png(from provider: NSItemProvider) async -> Data? {
        if provider.hasItemConformingToTypeIdentifier(UTType.png.identifier) {
            return await withCheckedContinuation { done in
                _ = provider.loadDataRepresentation(for: .png) { data, _ in done.resume(returning: data) }
            }
        }
        guard provider.canLoadObject(ofClass: UIImage.self) else { return nil }
        return await withCheckedContinuation { done in
            _ = provider.loadObject(ofClass: UIImage.self) { image, _ in
                done.resume(returning: (image as? UIImage)?.pngData())
            }
        }
    }

    func fetch() {
        guard let client else { return }
        run("Checking your devices…") {
            let (latest, addresses) = try await client.latest()
            self.addresses = addresses
            guard let received = latest, received.id != self.lastClipID else {
                return .upToDate
            }
            let board = UIPasteboard.general
            switch received.content {
            case let .text(text): board.string = text
            case let .image(png): board.setItems([[UTType.png.identifier: png]])
            }
            self.lastClipID = received.id
            self.handledChangeCount = board.changeCount
            return .received(Self.preview(received.content), from: received.from)
        }
    }

    private var client: SyncClient? {
        group.map {
            // Typed-in addresses first, then any others that worked last time.
            let remembered = manualAddresses + addresses.filter { !manualAddresses.contains($0) }
            return SyncClient(group: $0, device: device, name: UIDevice.current.name, remembered: remembered, relay: relay)
        }
    }

    private func run(_ message: String, _ work: @escaping @MainActor () async throws -> Status) {
        status = .working(message)
        // Gives the transfer time to finish if the user leaves the app right away.
        let background = UIApplication.shared.beginBackgroundTask()
        running = Task {
            do {
                status = try await work()
                log.info("sync finished: \(String(describing: self.status), privacy: .private)")
            } catch SyncError.noDevices {
                status = .failed("No devices found. Make sure CopyBlip is running on your computer and you're on the same Wi-Fi.")
            } catch SyncError.unreachable {
                status = .failed("Couldn't connect to your computer. Make sure CopyBlip is running on it.")
            } catch SyncError.wrongCode {
                status = .failed("Your computer rejected this phone. Leave the group and join again with the current pairing code.")
            } catch SyncError.tooLarge {
                status = .failed("That's too big to share. The limits are 1 MB of text or a 16 MB image.")
            } catch {
                status = .failed(error.localizedDescription)
            }
            running = nil
            UIApplication.shared.endBackgroundTask(background)
        }
    }

    private static func preview(_ content: Content) -> Preview {
        switch content {
        case let .text(text):
            let line = text.trimmingCharacters(in: .whitespacesAndNewlines)
            return .text(line.count > 120 ? String(line.prefix(120)) + "…" : line)
        case let .image(png):
            return UIImage(data: png).map(Preview.image) ?? .text("An image")
        }
    }
}
