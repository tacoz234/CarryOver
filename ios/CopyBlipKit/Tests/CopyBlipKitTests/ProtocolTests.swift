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

@Test func imagesAndUnknownTypesDecodeLikeTheDesktop() throws {
    // Same JSON as images_travel_as_base64_and_unknown_types_are_tolerated in protocol.rs.
    let image = try JSONDecoder().decode(Message.self, from: Data(#"{"t":"image","id":"1","png":"AAEC/w=="}"#.utf8))
    #expect(image == .image(id: "1", png: Data([0, 1, 2, 255])))
    let future = try JSONDecoder().decode(Message.self, from: Data(#"{"t":"files","id":"2"}"#.utf8))
    #expect(future == .unknown)
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

@Test func relayRoomMatchesDesktop() {
    // Same value as relay_room_differs_from_group_tag in desktop/src/protocol.rs.
    #expect(GroupKey(bytes: Data(repeating: 1, count: 32)).relayRoom == "b516525745f11172a676fbd38a1be4e1")
}

@Test func relayURLsAreNormalized() {
    #expect(Relay(string: "https://relay.example.dev/")?.base.absoluteString == "https://relay.example.dev")
    #expect(Relay(string: "wss://relay.example.dev")?.base.absoluteString == "https://relay.example.dev")
    #expect(Relay(string: "http://127.0.0.1:8787")?.base.absoluteString == "http://127.0.0.1:8787")
    #expect(Relay(string: "relay.example.dev") == nil)
    #expect(Relay(string: "https://relay.example.dev/some/path") == nil)
    let group = GroupKey(bytes: Data(repeating: 1, count: 32))
    let pipe = Relay(string: "https://relay.example.dev")!.pipe(group: group, from: "phone", to: "mac")
    #expect(pipe.absoluteString == "wss://relay.example.dev/v1/rooms/b516525745f11172a676fbd38a1be4e1/dial?device=phone&to=mac")
}

@Test func typedAddressesGetTheDefaultPort() {
    #expect(Address(parsing: "macmini") == Address(host: "macmini", port: 47820))
    #expect(Address(parsing: " 100.64.0.1:5000 ") == Address(host: "100.64.0.1", port: 5000))
    #expect(Address(parsing: "fd7a:115c::1") == Address(host: "fd7a:115c::1", port: 47820))
    #expect(Address(parsing: "[fd7a:115c::1]:9") == Address(host: "fd7a:115c::1", port: 9))
    #expect(Address(parsing: "host:notaport") == nil)
    #expect(Address(parsing: "") == nil)
    #expect(Address(parsing: "https://example.com") == nil)
    #expect(Address(host: "fd7a::1", port: 9).description == "[fd7a::1]:9")
}

@Test func invitesParseLinksAndBareCodes() {
    let group = GroupKey(bytes: Data(repeating: 7, count: 32))
    let link = "copyblip://join?code=\(group.pairingCode)&relay=https://relay.example.dev"
    #expect(Invite(link)?.group == group)
    #expect(Invite(link)?.relay == Relay(string: "https://relay.example.dev"))
    #expect(Invite(" \(group.pairingCode)\n")?.relay == nil)
    #expect(Invite("copyblip://join?code=\(group.pairingCode)")?.relay == nil)
    #expect(Invite("copyblip://join?relay=https://relay.example.dev") == nil)
    #expect(Invite("copyblip://join?code=\(group.pairingCode)&relay=nonsense") == nil)
    #expect(Invite("https://example.com/join?code=\(group.pairingCode)") == nil)
}
