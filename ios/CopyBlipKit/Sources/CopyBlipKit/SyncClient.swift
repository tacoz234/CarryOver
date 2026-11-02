import Foundation
import Network

public struct Received: Equatable, Sendable {
    public let id: String
    public let text: String
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

    public init(group: GroupKey, device: String, name: String, remembered: [Address] = [], endpoints: EndpointSource? = nil) {
        self.group = group
        self.device = device
        self.name = name
        self.remembered = remembered
        self.endpoints = endpoints ?? { await Discovery.find(group: group) }
    }

    /// Sends `text` to every device reached. Also returns the addresses that worked.
    public func send(_ text: String) async throws -> (Sent, [Address]) {
        guard text.utf8.count <= maxText else { throw SyncError.tooLarge }
        let id = Data((0..<16).map { _ in UInt8.random(in: .min ... .max) }).hex
        // Every device gets the same ID, so they apply it once even though it
        // arrives from more than one direction.
        let clip = Message.clip(id: id, text: text)
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
            guard case let .clip(id, text)? = try? await withTimeout(wait, { try await peer.receive() }) else {
                return nil
            }
            return Received(id: id, text: text, from: peer.peerName)
        }
        // Devices in a group hold the same latest clip, so the first answer will do.
        return (reached.lazy.compactMap(\.value).first, reached.compactMap(\.address))
    }

    struct Reached<T: Sendable>: Sendable {
        let value: T
        let name: String
        let address: Address?
    }

    /// Runs `body` against each device. Tries remembered addresses first and
    /// only searches the network if none of them answer.
    func reach<T: Sendable>(
        stopWhen: @escaping @Sendable (T) -> Bool = { _ in false },
        _ body: @escaping @Sendable (PeerConnection) async throws -> T
    ) async throws -> [Reached<T>] {
        var failures: [Error] = []
        if !remembered.isEmpty {
            // Short timeout, because a stale address shouldn't delay the search.
            let (reached, failed) = await each(remembered.compactMap(\.endpoint), timeout: .milliseconds(1500), stopWhen: stopWhen, body)
            if !reached.isEmpty { return reached }
            failures += failed
        }
        let found = await endpoints()
        let (reached, failed) = await each(found, timeout: .seconds(4), stopWhen: stopWhen, body)
        if !reached.isEmpty { return reached }
        failures += failed
        if failures.contains(where: { ($0 as? ProtocolError) == .authenticationFailed }) {
            throw SyncError.wrongCode
        }
        throw found.isEmpty ? SyncError.noDevices : SyncError.unreachable
    }

    private func each<T: Sendable>(
        _ endpoints: [NWEndpoint],
        timeout: Duration,
        stopWhen: @escaping @Sendable (T) -> Bool,
        _ body: @escaping @Sendable (PeerConnection) async throws -> T
    ) async -> ([Reached<T>], [Error]) {
        await withTaskGroup(of: Result<Reached<T>, Error>.self) { tasks in
            for endpoint in endpoints {
                tasks.addTask { [self] in
                    let peer = PeerConnection(endpoint: endpoint)
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
