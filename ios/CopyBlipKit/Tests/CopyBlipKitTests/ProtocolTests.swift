@testable import CopyBlipKit
import Foundation
import Testing

@Test func groupTagMatchesDesktop() {
    // Same value as group_tag_matches_reference_value in desktop/src/protocol.rs.
    #expect(GroupKey(bytes: Data(repeating: 1, count: 32)).tag == "22953167989813ac")
}

@Test func pairingCodeRoundTripsAndRejectsBadInput() {
    let key = GroupKey(bytes: Data((0..<32).map { UInt8($0) }))
    #expect(GroupKey(pairingCode: key.pairingCode) == key)
    #expect(GroupKey(pairingCode: " " + key.pairingCode.uppercased() + "\n") == key)
    #expect(GroupKey(pairingCode: "abc") == nil)
    #expect(GroupKey(pairingCode: String(repeating: "zz", count: 32)) == nil)
}

@Test func messagesUseTheDesktopJSONShape() throws {
    let encoded = try JSONEncoder().encode(Message.latest)
    #expect(String(decoding: encoded, as: UTF8.self) == #"{"t":"latest"}"#)
    let clip = try JSONDecoder().decode(Message.self, from: Data(#"{"t":"clip","id":"a","text":"hi"}"#.utf8))
    #expect(clip == .clip(id: "a", text: "hi"))
}

private func session(key: GroupKey = GroupKey(bytes: Data(repeating: 7, count: 32))) throws -> (Sealer, Opener, Sealer, Opener) {
    let (ni, nr) = (Handshake.newNonce(), Handshake.newNonce())
    let (si, oi) = try Handshake.keys(group: key, ourNonce: ni, peerHello: Handshake.hello(nonce: nr), role: .initiator)
    let (sr, or) = try Handshake.keys(group: key, ourNonce: nr, peerHello: Handshake.hello(nonce: ni), role: .responder)
    return (si, oi, sr, or)
}

private func body(_ frame: Data) throws -> Data {
    let length = try Opener.frameLength(header: frame.prefix(4))
    #expect(length == frame.count - 4)
    return frame.dropFirst(4)
}

@Test func framesRoundTripAndRejectTamperingAndReplay() throws {
    var (initiator, _, _, responder) = try session()
    let frame = try body(initiator.seal(.clip(id: "1", text: "héllo 🌍")))
    var tampered = Data(frame)
    tampered[tampered.startIndex] ^= 1
    var copy = responder
    #expect(throws: ProtocolError.authenticationFailed) { try copy.open(tampered) }
    #expect(try responder.open(frame) == .clip(id: "1", text: "héllo 🌍"))
    #expect(throws: ProtocolError.authenticationFailed) { try responder.open(frame) }
}

@Test func ownFramesCannotBeReflected() throws {
    var (initiator, initiatorOpener, _, _) = try session()
    let frame = try body(initiator.seal(.ping))
    #expect(throws: ProtocolError.authenticationFailed) { try initiatorOpener.open(frame) }
}

@Test func nonCopyBlipPeerIsRejected() {
    #expect(throws: ProtocolError.notCopyBlip) {
        try Handshake.keys(group: GroupKey(bytes: Data(count: 32)), ourNonce: Handshake.newNonce(), peerHello: Data(count: 37), role: .initiator)
    }
}
