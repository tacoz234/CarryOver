import Foundation
import Network

public enum ConnectionError: Error {
    case closed
    case timedOut
}

/// One authenticated connection to a desktop agent. The phone always dials.
public final class PeerConnection: @unchecked Sendable {
    private let connection: NWConnection
    private let queue = DispatchQueue(label: "copyblip.peer")
    private var sealer: Sealer?
    private var opener: Opener?
    public private(set) var peerName = ""
    /// The resolved address once connected, for dialing directly next time.
    public private(set) var address: Address?

    public init(endpoint: NWEndpoint) {
        connection = NWConnection(to: endpoint, using: .tcp)
    }

    /// Connects, handshakes and swaps hellos. Gives up after `timeout`.
    public func open(group: GroupKey, device: String, name: String, timeout: Duration = .seconds(4)) async throws {
        try await withTimeout(timeout) { [self] in
            try await waitUntilReady()
            let nonce = Handshake.newNonce()
            try await write(Handshake.hello(nonce: nonce))
            let peerHello = try await read(exactly: Handshake.helloLength)
            (sealer, opener) = try Handshake.keys(group: group, ourNonce: nonce, peerHello: peerHello, role: .initiator)
            try await send(.hello(device: device, name: name))
            // Nothing is shared until the peer proves it holds the group key.
            guard case let .hello(_, peerName) = try await receive() else {
                throw ProtocolError.unexpectedMessage
            }
            self.peerName = peerName
            if case let .hostPort(host, port)? = connection.currentPath?.remoteEndpoint {
                // Drops the interface suffix, e.g. "%en0", which is only valid on this connection.
                let name = "\(host)".split(separator: "%").first.map(String.init) ?? "\(host)"
                address = Address(host: name, port: port.rawValue)
            }
        }
    }

    public func send(_ message: Message) async throws {
        guard var sealer else { throw ConnectionError.closed }
        let frame = try sealer.seal(message)
        self.sealer = sealer
        try await write(frame)
    }

    /// Next message other than a keepalive ping.
    public func receive() async throws -> Message {
        while true {
            guard var opener else { throw ConnectionError.closed }
            let length = try Opener.frameLength(header: try await read(exactly: 4))
            let message = try opener.open(try await read(exactly: length))
            self.opener = opener
            if message != .ping { return message }
        }
    }

    /// Sends anything queued, then closes.
    public func finish() async {
        try? await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, Error>) in
            connection.send(content: nil, contentContext: .finalMessage, isComplete: true, completion: .contentProcessed { error in
                if let error { done.resume(throwing: error) } else { done.resume() }
            })
        }
        close()
    }

    public func close() {
        connection.cancel()
    }

    private func waitUntilReady() async throws {
        let once = Once()
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (ready: CheckedContinuation<Void, Error>) in
                connection.stateUpdateHandler = { state in
                    switch state {
                    case .ready: once.run { ready.resume() }
                    case let .failed(error), let .waiting(error): once.run { ready.resume(throwing: error) }
                    case .cancelled: once.run { ready.resume(throwing: ConnectionError.closed) }
                    default: break
                    }
                }
                connection.start(queue: queue)
            }
        } onCancel: { [connection] in
            connection.cancel()
        }
    }

    private func write(_ data: Data) async throws {
        try await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, Error>) in
            connection.send(content: data, completion: .contentProcessed { error in
                if let error { done.resume(throwing: error) } else { done.resume() }
            })
        }
    }

    private func read(exactly count: Int) async throws -> Data {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (done: CheckedContinuation<Data, Error>) in
                connection.receive(minimumIncompleteLength: count, maximumLength: count) { data, _, isComplete, error in
                    if let error {
                        done.resume(throwing: error)
                    } else if let data, data.count == count {
                        done.resume(returning: data)
                    } else {
                        done.resume(throwing: isComplete ? ConnectionError.closed : ProtocolError.notCopyBlip)
                    }
                }
            }
        } onCancel: { [connection] in
            connection.cancel()
        }
    }
}

/// Runs `body`, cancelling it if it takes longer than `limit`.
func withTimeout<T: Sendable>(_ limit: Duration, _ body: @escaping @Sendable () async throws -> T) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask { try await body() }
        group.addTask {
            try await Task.sleep(for: limit)
            throw ConnectionError.timedOut
        }
        defer { group.cancelAll() }
        return try await group.next()!
    }
}

/// Guards a continuation that several callbacks may try to resume.
final class Once: @unchecked Sendable {
    private let lock = NSLock()
    private var done = false

    func run(_ action: () -> Void) {
        lock.lock()
        let first = !done
        done = true
        lock.unlock()
        if first { action() }
    }
}
