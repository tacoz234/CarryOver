import Foundation

/// A `carryover://join?code=...` link, shown as a QR code by `copyblip invite`.
/// Also accepts `copyblip://` links from before the rename, and a bare
/// pairing code. Other parts of a link, like the relay older versions added,
/// are ignored.
public struct Invite: Sendable, Equatable {
    public let group: GroupKey

    public init?(_ text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if let group = GroupKey(pairingCode: text) {
            self.group = group
            return
        }
        guard let parts = URLComponents(string: text), ["carryover", "copyblip"].contains(parts.scheme), parts.host == "join",
              let code = parts.queryItems?.first(where: { $0.name == "code" })?.value,
              let group = GroupKey(pairingCode: code) else { return nil }
        self.group = group
    }
}
