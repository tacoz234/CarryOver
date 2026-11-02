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
}

/// A device address that worked, so the next sync can skip discovery.
public struct Address: Codable, Hashable, Sendable {
    public let host: String
    public let port: UInt16

    public init(host: String, port: UInt16) {
        self.host = host
        self.port = port
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
    let endpoints: EndpointSource
    let relay: Relay?

    /// `remembered` holds addresses to try before searching the network, such
    /// as ones that worked last time and ones typed in for Tailscale.
    public init(
        group: GroupKey,
        device: String,
        name: String,
        remembered: [Address] = [],
        relay: Relay? = nil,
        endpoints: EndpointSource? = nil
    ) {
        self.group = group
        self.device = device
        self.name = name
        self.remembered = remembered
        self.relay = relay
        self.endpoints = endpoints ?? { await Discovery.find(group: group) }
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
        return (Sent(id: id, to: reached.map(\.name)), reached.compactMap(\.address))
    }

    /// The most recent clip any device has, or nil if none has copied anything
    /// yet. Also returns the addresses that worked.
    public func latest(wait: Duration = .seconds(2)) async throws -> (Received?, [Address]) {
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
        return (reached.lazy.compactMap(\.value).first, reached.compactMap(\.address))
    }

    struct Reached<T: Sendable>: Sendable {
        let value: T
        let name: String
        let address: Address?
    }

    enum Target: Sendable {
        case direct(NWEndpoint)
        case relayed(URL)

        func connection() -> PeerConnection {
            switch self {
            case let .direct(endpoint): PeerConnection(endpoint: endpoint)
            case let .relayed(pipe): PeerConnection(pipe: pipe)
            }
        }
    }

    struct Attempt<T: Sendable>: Sendable {
        var reached: [Reached<T>] = []
        var failures: [Error] = []
        var foundAny = false
    }

    /// How long to try direct connections before also trying the relay.
    static let relayDelay: Duration = .milliseconds(800)

    /// Runs `body` against each device. Tries direct connections first, and
    /// the relay if those haven't worked shortly after.
    func reach<T: Sendable>(
        stopWhen: @escaping @Sendable (T) -> Bool = { _ in false },
        _ body: @escaping @Sendable (PeerConnection) async throws -> T
    ) async throws -> [Reached<T>] {
        let combined = await withTaskGroup(of: Attempt<T>.self) { tasks in
            tasks.addTask { await self.direct(stopWhen: stopWhen, body) }
            if let relay {
                tasks.addTask {
                    guard (try? await Task.sleep(for: Self.relayDelay)) != nil else { return Attempt() }
                    return await self.relayed(relay, stopWhen: stopWhen, body)
                }
            }
            var combined = Attempt<T>()
            for await attempt in tasks {
                if !attempt.reached.isEmpty {
                    // The other path is no longer needed.
                    tasks.cancelAll()
                    return attempt
                }
                combined.failures += attempt.failures
                combined.foundAny = combined.foundAny || attempt.foundAny
            }
            return combined
        }
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
            let targets = remembered.compactMap(\.endpoint).map(Target.direct)
            (attempt.reached, attempt.failures) = await each(targets, timeout: .milliseconds(1500), stopWhen: stopWhen, body)
            attempt.foundAny = !attempt.reached.isEmpty
            if !attempt.reached.isEmpty || Task.isCancelled { return attempt }
        }
        let found = await endpoints()
        let (reached, failed) = await each(found.map(Target.direct), timeout: .seconds(4), stopWhen: stopWhen, body)
        attempt.reached = reached
        attempt.failures += failed
        attempt.foundAny = attempt.foundAny || !found.isEmpty
        return attempt
    }

    private func relayed<T: Sendable>(
        _ relay: Relay,
        stopWhen: @escaping @Sendable (T) -> Bool,
        _ body: @escaping @Sendable (PeerConnection) async throws -> T
    ) async -> Attempt<T> {
        guard let devices = try? await relay.devices(group: group) else { return Attempt() }
        let targets = devices.map { Target.relayed(relay.pipe(group: group, from: device, to: $0)) }
        let (reached, failed) = await each(targets, timeout: .seconds(6), stopWhen: stopWhen, body)
        return Attempt(reached: reached, failures: failed, foundAny: !devices.isEmpty)
    }

    private func each<T: Sendable>(
        _ targets: [Target],
        timeout: Duration,
        stopWhen: @escaping @Sendable (T) -> Bool,
        _ body: @escaping @Sendable (PeerConnection) async throws -> T
    ) async -> ([Reached<T>], [Error]) {
        await withTaskGroup(of: Result<Reached<T>, Error>.self) { tasks in
            for target in targets {
                tasks.addTask { [self] in
                    let peer = target.connection()
                    defer { peer.close() }
                    do {
                        try await peer.open(group: group, device: device, name: name, timeout: timeout)
                        let value = try await body(peer)
                        return .success(Reached(value: value, name: peer.peerName, address: peer.address))
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
