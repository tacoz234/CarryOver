import CopyBlipKit
import Observation
import Security
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

    /// A device this iPhone has synced with, for the device grid.
    struct Device: Codable, Hashable, Identifiable {
        /// As the device reports it, such as coles-Mac-Mini.local.
        let name: String
        var lastSeen: Date
        /// Its device ID, which computers need to add it to their lists.
        /// Missing from lists saved by older versions until it's seen again.
        var device: String?
        var id: String { name }
    }

    /// A clip this iPhone sent or received, for the Recent list.
    struct HistoryItem: Codable, Identifiable, Equatable {
        enum Direction: String, Codable { case sent, received }
        let id: String
        let date: Date
        let direction: Direction
        let peers: [String]
        /// Nil for images, which are saved as files named after the ID.
        let text: String?
    }

    private(set) var pasteCheck = PasteAccessCheck.notChecked
    private(set) var localConnectionVerified = false
    private(set) var verifyingGroupConnection = false
    /// Prevent a check's system alert from starting an automatic sync.
    private var checkingPaste = false
    var permissionChecklistVisible = false
    private(set) var status = Status.idle
    private(set) var group: GroupKey?
    var showingSetup = false
    private(set) var needsOnboarding = false

    /// Most recently seen first.
    private(set) var devices: [Device] = [] {
        didSet { defaults.set(try? JSONEncoder().encode(devices), forKey: "devices") }
    }

    /// Devices that answered the last sync, or are connected to a computer
    /// that did. Nil until a sync has finished, since it isn't known yet.
    private(set) var online: Set<String>?

    /// Newest first, up to `historyLimit`.
    private(set) var history: [HistoryItem] = [] {
        didSet { try? JSONEncoder().encode(history).write(to: Self.historyIndex, options: .atomic) }
    }

    private static let historyLimit = 20
    private static let historyFolder: URL = {
        let folder = URL.applicationSupportDirectory.appending(path: "History", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }()
    private static var historyIndex: URL { historyFolder.appending(path: "history.json") }
    private static func imageFile(_ id: String) -> URL { historyFolder.appending(path: "\(id).png") }
    /// Decoded once, since full-size images can be several megabytes.
    @ObservationIgnored private var thumbnails: [String: UIImage] = [:]
    private var running: Task<Void, Never>?
    /// System alerts, including the paste prompt, make the app inactive and then
    /// active again. Only a real open, from launch or the background, should sync.
    private var syncedSinceOpen = false

    private let defaults = UserDefaults.standard
    private let device: String

    init() {
        #if DEBUG
        // Keychain items survive uninstalling. Used explicitly by device tests
        // after reinstalling to exercise the first-run wizard with no pairing.
        if ProcessInfo.processInfo.arguments.contains("--reset-pairing-for-testing") {
            Keychain.delete()
            precondition(Keychain.load() == nil, "Couldn't clear the saved pairing for testing")
        }
        #endif
        if let saved = defaults.string(forKey: "device") {
            device = saved
        } else {
            device = UUID().uuidString.lowercased()
            defaults.set(device, forKey: "device")
        }
        group = Keychain.load().flatMap { GroupKey(bytes: $0) as GroupKey? }
        needsOnboarding = group == nil || defaults.bool(forKey: "onboardingPending")
        // Left by versions that could sync away from home.
        defaults.removeObject(forKey: "manualAddresses")
        defaults.removeObject(forKey: "relay")
        devices = defaults.data(forKey: "devices")
            .flatMap { try? JSONDecoder().decode([Device].self, from: $0) } ?? []
        history = (try? Data(contentsOf: Self.historyIndex))
            .flatMap { try? JSONDecoder().decode([HistoryItem].self, from: $0) } ?? []
        #if DEBUG
        // Lets scripts pair a simulator without typing the code.
        if let code = ProcessInfo.processInfo.environment["COPYBLIP_PAIRING_CODE"] {
            // Scripts can't tap through the setup guide.
            defaults.set(true, forKey: "setupShown")
            defaults.set(true, forKey: "permissionChecklistShown")
            needsOnboarding = false
            if group != GroupKey(pairingCode: code) { _ = join(code: code) }
        }
        #endif
        if group != nil && !needsOnboarding { showSetupOnce() }
    }

    private func showSetupOnce() {
        guard !needsOnboarding, !defaults.bool(forKey: "permissionChecklistShown") else { return }
        defaults.set(true, forKey: "setupShown")
        defaults.set(true, forKey: "permissionChecklistShown")
        showingSetup = true
    }

    /// The pasteboard change count when we last sent or wrote it. Reading the
    /// count doesn't trigger the paste prompt; reading the contents does.
    private var handledChangeCount: Int? {
        get { defaults.object(forKey: "handledChangeCount") as? Int }
        set { defaults.set(newValue, forKey: "handledChangeCount") }
    }

    /// A copy the user wouldn't let Carryover paste. It isn't asked about
    /// again, or the prompt and setup guide would keep coming back.
    private var refusedChangeCount: Int? {
        get { defaults.object(forKey: "refusedChangeCount") as? Int }
        set { defaults.set(newValue, forKey: "refusedChangeCount") }
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

    /// An invite opened from a link, waiting for the user to confirm.
    var pendingInvite: Invite?

    func open(_ url: URL) {
        // The confirmation can't appear over the setup guide.
        showingSetup = false
        pendingInvite = Invite(url.absoluteString)
        if pendingInvite == nil {
            status = .failed("That Carryover link isn't valid. Make a new one with `copyblip invite`.")
        }
    }

    func acceptInvite() {
        guard let invite = pendingInvite else { return }
        pendingInvite = nil
        _ = join(invite.group, remembered: invite.addresses)
    }

    /// Accepts an invite link or a bare pairing code.
    func join(code: String) -> Bool {
        guard let invite = Invite(code) else { return false }
        return join(invite.group, remembered: invite.addresses)
    }

    private func join(_ key: GroupKey, remembered: [Address] = []) -> Bool {
        guard Keychain.save(key.bytes) else { return false }
        group = key
        if needsOnboarding { defaults.set(true, forKey: "onboardingPending") }
        lastClipID = nil
        syncedSinceOpen = false
        addresses = remembered
        showSetupOnce()
        // Whatever is on the clipboard now predates joining, so don't send it.
        handledChangeCount = UIPasteboard.general.changeCount
        status = .idle
        return true
    }

    /// Generate the group secret locally, just as desktop setup does.
    func createGroup() -> Bool {
        var bytes = Data(count: GroupKey.length)
        let result = bytes.withUnsafeMutableBytes { buffer in
            SecRandomCopyBytes(kSecRandomDefault, buffer.count, buffer.baseAddress!)
        }
        guard result == errSecSuccess else { return false }
        return join(GroupKey(bytes: bytes))
    }

    /// Checks the group's authenticated connection without touching clipboard data.
    func verifyGroupConnection() async throws -> [String] {
        guard let client else { return [] }
        verifyingGroupConnection = true
        defer { verifyingGroupConnection = false }
        do {
            let (seen, verified) = try await client.connect()
            addresses = verified
            saw(seen)
            return seen.reached
        } catch {
            localConnectionVerified = false
            throw error
        }
    }

    /// An explicit read test, never a silent probe. Discard the content and
    /// remember only whether this read succeeded, not the persistent iOS setting.
    func testPasteAccess() {
        let board = UIPasteboard.general
        checkingPaste = true
        defer { checkingPaste = false }
        pasteCheck = PasteAccessCheck.test(
            hasText: board.hasStrings,
            cameFromCarryover: defaults.object(forKey: "appWrittenChangeCount") as? Int == board.changeCount,
            canReadText: { board.string != nil }
        )
    }

    func resetPermissionChecks() {
        pasteCheck = .notChecked
        localConnectionVerified = false
    }

    private func markAppWrite() {
        defaults.set(UIPasteboard.general.changeCount, forKey: "appWrittenChangeCount")
    }

    func finishOnboarding() {
        guard group != nil else { return }
        defaults.set(true, forKey: "setupShown")
        defaults.set(true, forKey: "permissionChecklistShown")
        defaults.removeObject(forKey: "onboardingPending")
        needsOnboarding = false
        setupClosed()
    }

    func leave() {
        Keychain.delete()
        group = nil
        localConnectionVerified = false
        needsOnboarding = true
        defaults.removeObject(forKey: "onboardingPending")
        addresses = []
        devices = []
        clearHistory()
        status = .idle
    }

    /// The link that adds a device to this group, as a QR code or pasted.
    var inviteLink: String? {
        group.map { key in
            let peers = addresses.filter { Address(inviteEndpoint: "\($0.host):\($0.port)") != nil }
                .prefix(16).map { "&peer=\($0.host):\($0.port)" }.joined()
            return "carryover://join?code=\(key.pairingCode)\(peers)"
        }
    }

    func forgetDevices(at offsets: IndexSet) {
        devices.remove(atOffsets: offsets)
    }

    private func record(_ direction: HistoryItem.Direction, _ content: Content, id: String, peers: [String]) {
        let now = Date()
        for name in peers {
            let id = devices.first { $0.name == name }?.device
            devices.removeAll { $0.name == name }
            devices.insert(Device(name: name, lastSeen: now, device: id), at: 0)
        }
        var text: String?
        switch content {
        case let .text(value): text = value
        case let .image(png): try? png.write(to: Self.imageFile(id), options: .atomic)
        }
        var items = history.filter { $0.id != id }
        items.insert(HistoryItem(id: id, date: now, direction: direction, peers: peers, text: text), at: 0)
        for old in items.dropFirst(Self.historyLimit) { removeFiles(of: old) }
        history = Array(items.prefix(Self.historyLimit))
    }

    private func removeFiles(of item: HistoryItem) {
        try? FileManager.default.removeItem(at: Self.imageFile(item.id))
        thumbnails[item.id] = nil
    }

    func delete(_ item: HistoryItem) {
        removeFiles(of: item)
        history.removeAll { $0.id == item.id }
    }

    func clearHistory() {
        history.forEach(removeFiles)
        history = []
    }

    /// A small copy of an image clip, or nil for text.
    func thumbnail(for item: HistoryItem) -> UIImage? {
        guard item.text == nil else { return nil }
        if let cached = thumbnails[item.id] { return cached }
        // Loaded through Data: `path()` percent-encodes the space in
        // "Application Support", so the file would never be found.
        guard let png = try? Data(contentsOf: Self.imageFile(item.id)), let image = UIImage(data: png) else { return nil }
        let scale = min(1, 600 / max(image.size.width, image.size.height, 1))
        let size = CGSize(width: image.size.width * scale, height: image.size.height * scale)
        let thumbnail = image.preparingThumbnail(of: size) ?? image
        thumbnails[item.id] = thumbnail
        return thumbnail
    }

    /// Puts a past clip back on the clipboard. Marked as handled, so opening
    /// the app doesn't send it out again.
    func copy(_ item: HistoryItem) {
        let board = UIPasteboard.general
        if let text = item.text {
            board.string = text
        } else if let png = try? Data(contentsOf: Self.imageFile(item.id)) {
            board.setItems([[UTType.png.identifier: png]])
        } else {
            return
        }
        handledChangeCount = board.changeCount
        markAppWrite()
    }

    /// Copies text, such as the invite link, without sending it to the group.
    func copyWithoutSending(_ text: String) {
        UIPasteboard.general.string = text
        handledChangeCount = UIPasteboard.general.changeCount
        markAppWrite()
    }

    func appClosed() {
        syncedSinceOpen = false
    }

    /// Sends the clipboard if it changed since last time, otherwise fetches the
    /// latest clip from the other devices.
    func appOpened() {
        guard !permissionChecklistVisible, !checkingPaste, !needsOnboarding, group != nil, !syncedSinceOpen, running == nil, !showingSetup else { return }
        syncedSinceOpen = true
        let board = UIPasteboard.general
        if board.changeCount != handledChangeCount, board.hasStrings || board.hasImages {
            // Neither sends nor fetches, so a copy the user hasn't shared isn't
            // overwritten. Send Clipboard and Get Latest still work.
            guard board.changeCount != refusedChangeCount else {
                log.info("paste was refused for this copy already, not asking again")
                return
            }
            log.info("clipboard changed since last sync, sending")
            // This read is what shows "Allow Paste" unless the user allowed it in Settings.
            guard let content = Self.read(board) else {
                log.info("paste was refused")
                refusedChangeCount = board.changeCount
                status = .failed("Carryover wasn't allowed to paste, so nothing was sent.")
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
            self.saw(sent.seen)
            self.lastClipID = sent.id
            self.record(.sent, content, id: sent.id, peers: sent.to)
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
            let (latest, addresses, seen) = try await client.latest()
            self.addresses = addresses
            self.saw(seen)
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
            self.markAppWrite()
            self.record(.received, received.content, id: received.id, peers: [received.from])
            return .received(Self.preview(received.content), from: received.from)
        }
    }

    private var client: SyncClient? {
        group.map { SyncClient(group: $0, device: device, name: UIDevice.current.name, remembered: addresses, known: known) }
    }

    /// The devices this iPhone knows about, as told to computers. It isn't
    /// connected to any of them between syncs.
    private var known: [Presence] {
        devices.compactMap { item in
            item.device.map { Presence(device: $0, name: item.name, connected: false, lastSeen: item.lastSeen) }
        }
    }

    /// Updates who's online, and adds devices the computers know about that
    /// this iPhone hasn't synced with yet.
    private func saw(_ seen: Seen) {
        localConnectionVerified = !seen.reached.isEmpty
        let others = seen.devices.filter { $0.device != device }
        online = Set(seen.reached + others.filter(\.connected).map(\.name))
        let now = Date()
        var merged = devices
        for name in seen.reached {
            let id = seen.reachedIDs[name] ?? merged.first { $0.name == name }?.device
            merged.removeAll { $0.name == name }
            merged.append(Device(name: name, lastSeen: now, device: id))
        }
        for report in others {
            let lastSeen = report.connected ? now : report.lastSeen
            if let index = merged.firstIndex(where: { $0.name == report.name }) {
                merged[index].lastSeen = max(merged[index].lastSeen, lastSeen)
                merged[index].device = merged[index].device ?? report.device
            } else {
                merged.append(Device(name: report.name, lastSeen: lastSeen, device: report.device))
            }
        }
        merged.sort { $0.lastSeen > $1.lastSeen }
        if merged != devices { devices = merged }
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
                localConnectionVerified = false
                // Nothing answered, so nothing is known to be online.
                online = []
                status = .failed("No devices found. Make sure Carryover is running on your computer and you're on the same Wi-Fi.")
            } catch SyncError.unreachable {
                localConnectionVerified = false
                online = []
                status = .failed("Couldn't connect to your computer. Make sure Carryover is running on it.")
            } catch SyncError.wrongCode {
                online = []
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
