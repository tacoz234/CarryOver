import Foundation
import Network

public struct Received: Equatable, Sendable {
    public let id: String
    public let content: Content
    public let from: String
}

public struct Sent: Equatable, Sendable {
    public let id: String
    public let to: [String]
    public let seen: Seen
}

/// Who answered a sync, and what they said about the rest of the group.
public struct Seen: Equatable, Sendable {
    /// Names of the computers this iPhone reached.
    public let reached: [String]
    /// The devices those computers know about, one entry each. Connected if
    /// any of them is connected to it.
    public let devices: [Presence]
    /// Device IDs of the computers reached, by name.
    public let reachedIDs: [String: String]

    init(reached: [String], reports: [[Presence]], reachedIDs: [String: String] = [:]) {
        self.reached = reached
        self.reachedIDs = reachedIDs
        var merged: [String: Presence] = [:]
        for report in reports.joined() {
            guard let existing = merged[report.device] else {
                merged[report.device] = report
                continue
            }
            merged[report.device] = Presence(
                device: report.device,
                name: report.lastSeen > existing.lastSeen ? report.name : existing.name,
                connected: existing.connected || report.connected,
                lastSeen: max(existing.lastSeen, report.lastSeen)
            )
        }
        devices = merged.values.sorted { $0.name < $1.name }
    }

    init<T>(_ reached: [SyncClient.Reached<T>]) {
        self.init(
            reached: reached.map(\.name),
            reports: reached.map(\.reported),
            reachedIDs: Dictionary(reached.map { ($0.name, $0.device) }, uniquingKeysWith: { first, _ in first })
        )
    }

    /// Whether `name` answered, or is connected to a computer that did.
    public func isOnline(_ name: String) -> Bool {
        reached.contains(name) || devices.contains { $0.name == name && $0.connected }
    }
}

/// A device address that worked, so the next sync can skip discovery.
public struct Address: Codable, Hashable, Sendable {
    public let host: String
    public let port: UInt16

    public init(host: String, port: UInt16) {
        self.host = host
        self.port = port
    }

    /// Invites only carry IPv4 LAN endpoints, never public servers or URLs.
    public init?(inviteEndpoint: String) {
        let parts = inviteEndpoint.split(separator: ":", omittingEmptySubsequences: false)
        guard parts.count == 2, let port = UInt16(parts[1]), port > 0,
              let ip = IPv4Address(String(parts[0])) else { return nil }
        let bytes = Array(ip.rawValue)
        guard bytes[0] == 10 || (bytes[0] == 172 && (16...31).contains(bytes[1]))
            || (bytes[0] == 192 && bytes[1] == 168) || (bytes[0] == 169 && bytes[1] == 254) else { return nil }
        self.init(host: String(parts[0]), port: port)
    }

    var endpoint: NWEndpoint? {
        NWEndpoint.Port(rawValue: port).map { .hostPort(host: NWEndpoint.Host(host), port: $0) }
    }
}

/// What the app does when opened: push one clip out, or pull the latest one in.
public struct SyncClient: Sendable {
    public typealias EndpointSource = @Sendable () async -> [NWEndpoint]

    let group: GroupKey
    let device: String
    let name: String
    let remembered: [Address]
    /// The devices this iPhone knows about, told to each computer it reaches
    /// so a computer that joined through the iPhone learns the whole group.
    let known: [Presence]
    let endpoints: EndpointSource

    /// `remembered` holds addresses that worked last time, tried before
    /// searching the network.
    public init(
        group: GroupKey,
        device: String,
        name: String,
        remembered: [Address] = [],
        known: [Presence] = [],
        endpoints: EndpointSource? = nil
    ) {
        self.group = group
        self.device = device
        self.name = name
        self.remembered = remembered
        self.known = known
        self.endpoints = endpoints ?? { await Discovery.find(group: group) }
    }

    /// Proves a device in the group is reachable without sending or fetching
    /// clipboard contents. Used to check pairing independently from syncing.
    public func connect() async throws -> (Seen, [Address]) {
        let reached = try await reach { peer in
            await peer.finish()
            return true
        }
        return (Seen(reached), reached.compactMap(\.address))
    }

    /// Sends `content` to every device reached. Also returns the addresses that worked.
    public func send(_ content: Content) async throws -> (Sent, [Address]) {
        let limit = if case .image = content { maxImage } else { maxText }
        guard content.byteCount <= limit else { throw SyncError.tooLarge }
        let id = Data((0..<16).map { _ in UInt8.random(in: .min ... .max) }).hex
        // Every device gets the same ID, so they apply it once even though it
        // arrives from more than one direction.
        let clip = content.message(id: id)
        let reached = try await reach { peer in
            try await peer.send(clip)
            await peer.finish()
            return true
        }
        let seen = Seen(reached)
        return (Sent(id: id, to: reached.map(\.name), seen: seen), reached.compactMap(\.address))
    }

    /// The most recent clip any device has, or nil if none has copied anything
    /// yet. Also returns the addresses that worked, and who's online.
    public func latest(wait: Duration = .seconds(2)) async throws -> (Received?, [Address], Seen) {
        let reached = try await reach(stopWhen: { $0 != nil }) { peer -> Received? in
            try await peer.send(.latest)
            // No reply means the device hasn't seen a copy yet.
            switch try? await withTimeout(wait, { try await peer.receive() }) {
            case let .clip(id, text)?: return Received(id: id, content: .text(text), from: peer.peerName)
            case let .image(id, png)?: return Received(id: id, content: .image(png), from: peer.peerName)
            default: return nil
            }
        }
        // Devices in a group hold the same latest clip, so the first answer will do.
        let seen = Seen(reached)
        return (reached.lazy.compactMap(\.value).first, reached.compactMap(\.address), seen)
    }

    struct Reached<T: Sendable>: Sendable {
        let value: T
        let name: String
        let device: String
        let address: Address?
        let reported: [Presence]
    }

    struct Attempt<T: Sendable>: Sendable {
        var reached: [Reached<T>] = []
        var failures: [Error] = []
        var foundAny = false
    }

    /// Runs `body` against each device, trying remembered addresses first.
    func reach<T: Sendable>(
        stopWhen: @escaping @Sendable (T) -> Bool = { _ in false },
        _ body: @escaping @Sendable (PeerConnection) async throws -> T
    ) async throws -> [Reached<T>] {
        let combined = await direct(stopWhen: stopWhen, body)
        if !combined.reached.isEmpty { return combined.reached }
        if combined.failures.contains(where: { ($0 as? ProtocolError) == .authenticationFailed }) {
            throw SyncError.wrongCode
        }
        throw combined.foundAny ? SyncError.unreachable : SyncError.noDevices
    }

    private func direct<T: Sendable>(
        stopWhen: @escaping @Sendable (T) -> Bool,
        _ body: @escaping @Sendable (PeerConnection) async throws -> T
    ) async -> Attempt<T> {
        var attempt = Attempt<T>()
        if !remembered.isEmpty {
            // Short timeout, because a stale address shouldn't delay the search.
            let targets = remembered.compactMap(\.endpoint)
            (attempt.reached, attempt.failures) = await each(targets, timeout: .milliseconds(1500), stopWhen: stopWhen, body)
            attempt.foundAny = !attempt.reached.isEmpty
            if !attempt.reached.isEmpty || Task.isCancelled { return attempt }
        }
        let found = await endpoints()
        let (reached, failed) = await each(found, timeout: .seconds(4), stopWhen: stopWhen, body)
        attempt.reached = reached
        attempt.failures += failed
        attempt.foundAny = attempt.foundAny || !found.isEmpty
        return attempt
    }

    private func each<T: Sendable>(
        _ targets: [NWEndpoint],
        timeout: Duration,
        stopWhen: @escaping @Sendable (T) -> Bool,
        _ body: @escaping @Sendable (PeerConnection) async throws -> T
    ) async -> ([Reached<T>], [Error]) {
        await withTaskGroup(of: Result<Reached<T>, Error>.self) { tasks in
            for target in targets {
                tasks.addTask { [self] in
                    let peer = PeerConnection(endpoint: target)
                    defer { peer.close() }
                    do {
                        try await peer.open(group: group, device: device, name: name, devices: known, timeout: timeout)
                        let value = try await body(peer)
                        return .success(Reached(value: value, name: peer.peerName, device: peer.peerDevice, address: peer.address, reported: peer.reported))
                    } catch {
                        return .failure(error)
                    }
                }
            }
            var reached: [Reached<T>] = []
            var failed: [Error] = []
            for await result in tasks {
                switch result {
                case let .success(value):
                    reached.append(value)
                    if stopWhen(value.value) {
                        tasks.cancelAll()
                        return (reached, failed)
                    }
                case let .failure(error):
                    failed.append(error)
                }
            }
            return (reached, failed)
        }
    }
}

public enum SyncError: Error, Equatable {
    case noDevices
    case unreachable
    case wrongCode
    case tooLarge
}
