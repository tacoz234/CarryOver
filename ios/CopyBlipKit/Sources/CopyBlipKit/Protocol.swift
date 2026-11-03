// Wire protocol v1. Must match desktop/src/protocol.rs; see docs/protocol.md.
import CryptoKit
import Foundation

/// A device a computer knows about, for showing who's online.
public struct Presence: Codable, Equatable, Sendable {
    public let device: String
    public let name: String
    /// Whether the computer that sent this is connected to it right now.
    public let connected: Bool
    /// When that computer last saw it connected.
    public let lastSeen: Date

    public init(device: String, name: String, connected: Bool, lastSeen: Date) {
        self.device = device
        self.name = name
        self.connected = connected
        self.lastSeen = lastSeen
    }

    private enum CodingKeys: String, CodingKey {
        case device, name, connected
        case lastSeen = "last_seen"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        device = try c.decode(String.self, forKey: .device)
        name = try c.decode(String.self, forKey: .name)
        connected = try c.decode(Bool.self, forKey: .connected)
        // Seconds since 1970, as computers send it.
        lastSeen = Date(timeIntervalSince1970: try c.decode(Double.self, forKey: .lastSeen))
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(device, forKey: .device)
        try c.encode(name, forKey: .name)
        try c.encode(connected, forKey: .connected)
        try c.encode(UInt64(max(0, lastSeen.timeIntervalSince1970)), forKey: .lastSeen)
    }
}

public enum Message: Equatable, Sendable {
    /// Computers list the devices they know about. The phone sends none.
    case hello(device: String, name: String, devices: [Presence] = [])
    case clip(id: String, text: String)
    case image(id: String, png: Data)
    case latest
    /// A computer's devices changed while connected.
    case devices([Presence])
    case ping
    /// A message type from a newer version, skipped rather than ending the connection.
    case unknown
}

/// What a copy holds.
public enum Content: Equatable, Sendable {
    case text(String)
    /// PNG bytes.
    case image(Data)

    func message(id: String) -> Message {
        switch self {
        case let .text(text): .clip(id: id, text: text)
        case let .image(png): .image(id: id, png: png)
        }
    }

    var byteCount: Int {
        switch self {
        case let .text(text): text.utf8.count
        case let .image(png): png.count
        }
    }
}

extension Message: Codable {
    private enum Keys: String, CodingKey { case t, device, name, id, text, png, devices }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        switch try c.decode(String.self, forKey: .t) {
        case "hello":
            self = .hello(
                device: try c.decode(String.self, forKey: .device),
                name: try c.decode(String.self, forKey: .name),
                // Older computers leave it out.
                devices: try c.decodeIfPresent([Presence].self, forKey: .devices) ?? []
            )
        case "clip":
            self = .clip(id: try c.decode(String.self, forKey: .id), text: try c.decode(String.self, forKey: .text))
        case "image":
            // JSONDecoder reads Data as base64, matching the desktop.
            self = .image(id: try c.decode(String.self, forKey: .id), png: try c.decode(Data.self, forKey: .png))
        case "latest": self = .latest
        case "devices": self = .devices(try c.decode([Presence].self, forKey: .devices))
        case "ping": self = .ping
        default: self = .unknown
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case let .hello(device, name, devices):
            try c.encode("hello", forKey: .t)
            try c.encode(device, forKey: .device)
            try c.encode(name, forKey: .name)
            if !devices.isEmpty { try c.encode(devices, forKey: .devices) }
        case let .clip(id, text):
            try c.encode("clip", forKey: .t)
            try c.encode(id, forKey: .id)
            try c.encode(text, forKey: .text)
        case let .image(id, png):
            try c.encode("image", forKey: .t)
            try c.encode(id, forKey: .id)
            try c.encode(png, forKey: .png)
        case .latest: try c.encode("latest", forKey: .t)
        case let .devices(devices):
            try c.encode("devices", forKey: .t)
            try c.encode(devices, forKey: .devices)
        case .ping: try c.encode("ping", forKey: .t)
        case .unknown: throw EncodingError.invalidValue(self, .init(codingPath: [], debugDescription: "unknown messages can't be sent"))
        }
    }
}

public enum ProtocolError: Error, Equatable {
    case notCopyBlip
    case badFrameLength(Int)
    case authenticationFailed
    case unexpectedMessage
}

public struct GroupKey: Sendable, Equatable {
    public static let length = 32
    public let bytes: Data

    /// Accepts the hex code printed by `copyblip pairing-code`, ignoring spaces and dashes.
    public init?(pairingCode: String) {
        let hex = pairingCode.filter { !$0.isWhitespace && $0 != "-" }
        guard hex.count == Self.length * 2 else { return nil }
        var bytes = Data(capacity: Self.length)
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<next], radix: 16) else { return nil }
            bytes.append(byte)
            index = next
        }
        self.bytes = bytes
    }

    public init(bytes: Data) {
        precondition(bytes.count == Self.length)
        self.bytes = bytes
    }

    public var pairingCode: String { bytes.hex }

    /// The relay room for this group. Derived from the key, so only members can find it.
    public var relayRoom: String {
        HKDF<SHA256>.deriveKey(
            inputKeyMaterial: SymmetricKey(data: bytes),
            info: Data("copyblip v1 relay room".utf8),
            outputByteCount: 16
        ).withUnsafeBytes { Data($0) }.hex
    }

    /// Non-secret tag advertised over mDNS so devices can skip other groups.
    public var tag: String {
        let tag = HKDF<SHA256>.deriveKey(
            inputKeyMaterial: SymmetricKey(data: bytes),
            info: Data("copyblip v1 group tag".utf8),
            outputByteCount: 8
        )
        return tag.withUnsafeBytes { Data($0) }.hex
    }
}

public enum Role: Sendable { case initiator, responder }

public enum Handshake {
    static let magic = Data("CBLP".utf8) + [1]
    public static let helloLength = magic.count + 32

    public static func hello(nonce: Data) -> Data { magic + nonce }

    public static func newNonce() -> Data {
        Data((0..<32).map { _ in UInt8.random(in: .min ... .max) })
    }

    /// Derives one key per direction from both sides' random nonces.
    public static func keys(group: GroupKey, ourNonce: Data, peerHello: Data, role: Role) throws -> (Sealer, Opener) {
        guard peerHello.count == helloLength, peerHello.prefix(magic.count) == magic else {
            throw ProtocolError.notCopyBlip
        }
        let theirNonce = peerHello.suffix(32)
        let salt = role == .initiator ? ourNonce + theirNonce : theirNonce + ourNonce
        let okm = HKDF<SHA256>.deriveKey(
            inputKeyMaterial: SymmetricKey(data: group.bytes),
            salt: salt,
            info: Data("copyblip v1 session".utf8),
            outputByteCount: 64
        ).withUnsafeBytes { Data($0) }
        let toResponder = SymmetricKey(data: okm.prefix(32))
        let toInitiator = SymmetricKey(data: okm.suffix(32))
        return role == .initiator
            ? (Sealer(key: toResponder), Opener(key: toInitiator))
            : (Sealer(key: toInitiator), Opener(key: toResponder))
    }
}

/// Matches the desktop's limits. Leaves room for maxImage in base64, and
/// maxText after JSON escaping, which can grow text up to sixfold.
public let maxFrame = 24 * 1024 * 1024
public let maxText = 1024 * 1024
public let maxImage = 16 * 1024 * 1024
private let tagLength = 16

private func nonce(_ counter: UInt64) -> ChaChaPoly.Nonce {
    var bytes = Data(count: 4)
    withUnsafeBytes(of: counter.bigEndian) { bytes.append(contentsOf: $0) }
    return try! ChaChaPoly.Nonce(data: bytes)
}

public struct Sealer: Sendable {
    let key: SymmetricKey
    var counter: UInt64 = 0

    /// Returns the length-prefixed frame ready to write.
    public mutating func seal(_ message: Message) throws -> Data {
        let box = try ChaChaPoly.seal(JSONEncoder().encode(message), using: key, nonce: nonce(counter))
        counter += 1
        let sealed = box.ciphertext + box.tag
        guard sealed.count <= maxFrame else { throw ProtocolError.badFrameLength(sealed.count) }
        var frame = Data()
        withUnsafeBytes(of: UInt32(sealed.count).bigEndian) { frame.append(contentsOf: $0) }
        return frame + sealed
    }
}

public struct Opener: Sendable {
    let key: SymmetricKey
    var counter: UInt64 = 0

    public static func frameLength(header: Data) throws -> Int {
        let length = Int(header.reduce(UInt32(0)) { $0 << 8 | UInt32($1) })
        guard header.count == 4, (tagLength...maxFrame).contains(length) else {
            throw ProtocolError.badFrameLength(length)
        }
        return length
    }

    public mutating func open(_ sealed: Data) throws -> Message {
        let box: ChaChaPoly.SealedBox
        do {
            box = try ChaChaPoly.SealedBox(
                nonce: nonce(counter),
                ciphertext: sealed.dropLast(tagLength),
                tag: sealed.suffix(tagLength)
            )
        } catch {
            throw ProtocolError.authenticationFailed
        }
        guard let plaintext = try? ChaChaPoly.open(box, using: key) else {
            throw ProtocolError.authenticationFailed
        }
        counter += 1
        return try JSONDecoder().decode(Message.self, from: plaintext)
    }
}

extension Data {
    var hex: String { map { String(format: "%02x", $0) }.joined() }
}
