import CopyBlipKit
import Observation
import UIKit
import os

private let log = Logger(subsystem: "com.coledeterman.CopyBlip", category: "sync")

@MainActor
@Observable
final class SyncModel {
    enum Status: Equatable {
        case idle
        case working(String)
        case sent(preview: String, to: [String])
        case received(preview: String, from: String)
        case upToDate
        case failed(String)
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
        #if DEBUG
        // Lets scripts pair a simulator without typing the code.
        if let code = ProcessInfo.processInfo.environment["COPYBLIP_PAIRING_CODE"],
           group != GroupKey(pairingCode: code) {
            _ = join(code: code)
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

    func join(code: String) -> Bool {
        guard let key = GroupKey(pairingCode: code), Keychain.save(key.bytes) else { return false }
        group = key
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
        if board.changeCount != handledChangeCount, board.hasStrings {
            log.info("clipboard changed since last sync, sending")
            // This read is what shows "Allow Paste" unless the user allowed it in Settings.
            // Left unhandled so a later open retries instead of fetching over
            // a copy the user hasn't shared yet.
            guard let text = board.string else {
                log.info("paste was refused")
                status = .failed("CopyBlip wasn't allowed to paste, so nothing was sent.")
                showingSetup = true
                return
            }
            send(text)
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

    /// For the paste button, which never shows the paste prompt.
    func send(_ text: String) {
        guard let client else { return }
        handledChangeCount = UIPasteboard.general.changeCount
        run("Sending…") {
            let (sent, addresses) = try await client.send(text)
            self.addresses = addresses
            self.lastClipID = sent.id
            return .sent(preview: Self.preview(text), to: sent.to)
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
            board.string = received.text
            self.lastClipID = received.id
            self.handledChangeCount = board.changeCount
            return .received(preview: Self.preview(received.text), from: received.from)
        }
    }

    private var client: SyncClient? {
        group.map { SyncClient(group: $0, device: device, name: UIDevice.current.name, remembered: addresses) }
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
                status = .failed("That's too much text to share, the limit is 1 MB.")
            } catch {
                status = .failed(error.localizedDescription)
            }
            running = nil
            UIApplication.shared.endBackgroundTask(background)
        }
    }

    private static func preview(_ text: String) -> String {
        let line = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return line.count > 120 ? String(line.prefix(120)) + "…" : line
    }
}
