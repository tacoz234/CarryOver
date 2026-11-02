import Foundation

/// A CopyBlip relay, which connects devices on different networks. It only
/// passes encrypted bytes along. See docs/relay.md.
public struct Relay: Sendable, Equatable {
    /// The http or https form, without a trailing slash.
    public let base: URL

    /// Accepts https:// or wss://, and http:// or ws:// for local testing.
    public init?(string: String) {
        var text = string.trimmingCharacters(in: .whitespacesAndNewlines)
        while text.hasSuffix("/") { text.removeLast() }
        for (from, to) in [("wss://", "https://"), ("ws://", "http://")] where text.hasPrefix(from) {
            text = to + text.dropFirst(from.count)
        }
        guard let url = URL(string: text), ["https", "http"].contains(url.scheme), url.host() != nil,
              url.path().isEmpty || url.path() == "/" else { return nil }
        base = url
    }

    func room(_ group: GroupKey) -> URL {
        base.appending(path: "v1/rooms/\(group.relayRoom)")
    }

    /// Computers in the group that are connected to the relay.
    func devices(group: GroupKey) async throws -> [String] {
        var request = URLRequest(url: room(group).appending(path: "devices"))
        request.timeoutInterval = 5
        let (data, response) = try await URLSession.shared.data(for: request)
        guard (response as? HTTPURLResponse)?.statusCode == 200 else { throw ConnectionError.closed }
        return try JSONDecoder().decode([String].self, from: data)
    }

    /// Where to open a pipe from `device` to computer `to`.
    func pipe(group: GroupKey, from device: String, to: String) -> URL {
        var parts = URLComponents(url: room(group).appending(path: "dial"), resolvingAgainstBaseURL: false)!
        parts.scheme = base.scheme == "https" ? "wss" : "ws"
        parts.queryItems = [URLQueryItem(name: "device", value: device), URLQueryItem(name: "to", value: to)]
        return parts.url!
    }
}

extension Address {
    public static let defaultPort: UInt16 = 47820

    /// Accepts `host`, `host:port`, an IPv6 address, or `[ipv6]:port`, as
    /// typed in for Tailscale names and the like.
    public init?(parsing text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !text.contains("/"), !text.contains(" ") else { return nil }
        if text.hasPrefix("["), let close = text.firstIndex(of: "]") {
            let host = String(text[text.index(after: text.startIndex)..<close])
            let rest = text[text.index(after: close)...]
            guard !host.isEmpty else { return nil }
            if rest.isEmpty { self.init(host: host, port: Self.defaultPort); return }
            guard rest.hasPrefix(":"), let port = UInt16(rest.dropFirst()) else { return nil }
            self.init(host: host, port: port)
        } else if text.filter({ $0 == ":" }).count == 1 {
            let parts = text.split(separator: ":", omittingEmptySubsequences: false)
            guard !parts[0].isEmpty, let port = UInt16(parts[1]) else { return nil }
            self.init(host: String(parts[0]), port: port)
        } else {
            // A bare name, IPv4 address, or IPv6 address without brackets.
            self.init(host: text, port: Self.defaultPort)
        }
    }

    public var description: String {
        host.contains(":") ? "[\(host)]:\(port)" : "\(host):\(port)"
    }
}

/// A `copyblip://join?code=...&relay=...` link, shown as a QR code by
/// `copyblip invite`. Also accepts a bare pairing code.
public struct Invite: Sendable, Equatable {
    public let group: GroupKey
    public let relay: Relay?

    public init?(_ text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if let group = GroupKey(pairingCode: text) {
            self.group = group
            relay = nil
            return
        }
        guard let parts = URLComponents(string: text), parts.scheme == "copyblip", parts.host == "join",
              let code = parts.queryItems?.first(where: { $0.name == "code" })?.value,
              let group = GroupKey(pairingCode: code) else { return nil }
        let relayText = parts.queryItems?.first(where: { $0.name == "relay" })?.value
        // A relay that's present but invalid makes the whole link suspect.
        if let relayText, Relay(string: relayText) == nil { return nil }
        self.group = group
        relay = relayText.flatMap(Relay.init(string:))
    }
}
