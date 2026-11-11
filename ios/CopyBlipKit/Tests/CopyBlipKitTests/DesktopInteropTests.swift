// Runs against real desktop agents. Skipped unless COPYBLIP_INTEROP is set to
// "<pairing code> <port> <port>", which ios/scripts/interop.sh does.
@testable import CopyBlipKit
import Foundation
import Network
import Testing

struct Setup {
    let key: GroupKey
    let ports: [UInt16]

    static let current: Setup? = {
        let parts = (ProcessInfo.processInfo.environment["COPYBLIP_INTEROP"] ?? "").split(separator: " ")
        guard parts.count >= 2, let key = GroupKey(pairingCode: String(parts[0])) else { return nil }
        return Setup(key: key, ports: parts.dropFirst().compactMap { UInt16($0) })
    }()

    func client(ports: [UInt16]? = nil, key: GroupKey? = nil, remembered: [Address] = []) -> SyncClient {
        let endpoints = (ports ?? self.ports).map {
            NWEndpoint.hostPort(host: "127.0.0.1", port: NWEndpoint.Port(rawValue: $0)!)
        }
        return SyncClient(group: key ?? self.key, device: "test-phone", name: "Test iPhone", remembered: remembered, endpoints: { endpoints })
    }
}

@Suite(.enabled(if: Setup.current != nil), .serialized)
struct DesktopInteropTests {
    let setup = Setup.current!

    @Test(.enabled(if: !(ProcessInfo.processInfo.environment["COPYBLIP_INTEROP_HOST"] ?? "").isEmpty))
    func invitedPhoneConnectsWithoutBonjour() async throws {
        let host = ProcessInfo.processInfo.environment["COPYBLIP_INTEROP_HOST"]!
        let invite = try #require(Invite("carryover://join?code=\(setup.key.pairingCode)&peer=\(host):\(setup.ports[0])"))
        let client = SyncClient(group: invite.group, device: "invited-phone", name: "Invited iPhone",
            remembered: invite.addresses, endpoints: { [] })
        let (seen, addresses) = try await client.connect()
        #expect(seen.reached.count == 1)
        #expect(addresses.count == 1)
    }

    @Test func sendReachesEveryDesktop() async throws {
        let (sent, addresses) = try await setup.client().send(.text("from swift \(UUID())"))
        #expect(sent.to.count == setup.ports.count)
        #expect(Set(addresses.map(\.port)) == Set(setup.ports))
    }

    @Test func reachingOneDesktopShowsTheOtherOnline() async throws {
        // Both agents share a host name, so they're told apart by device ID.
        let (_, _, seen) = try await setup.client(ports: [setup.ports[0]]).latest()
        #expect(seen.reached.count == 1)
        #expect(seen.devices.contains { $0.connected && $0.device != "test-phone" })
    }

    @Test func computersLearnDevicesThePhoneKnows() async throws {
        // A computer that's off, which only the phone has met.
        let pc = Presence(device: "offline-pc", name: "Offline PC", connected: false, lastSeen: Date(timeIntervalSince1970: 1_700_000_000))
        let endpoint = NWEndpoint.hostPort(host: "127.0.0.1", port: NWEndpoint.Port(rawValue: setup.ports[0])!)
        let phone = SyncClient(group: setup.key, device: "phone-with-list", name: "Phone With List", known: [pc], endpoints: { [endpoint] })
        _ = try await phone.connect()
        let (seen, _) = try await setup.client(ports: [setup.ports[0]]).connect()
        let reported = try #require(seen.devices.first { $0.device == "offline-pc" })
        #expect(reported.name == "Offline PC")
        #expect(!reported.connected)
        #expect(seen.reachedIDs.values.allSatisfy { !$0.isEmpty })
    }

    @Test func latestReturnsWhatWasJustSent() async throws {
        let text = "round trip ✅ \(UUID())"
        let (sent, _) = try await setup.client(ports: [setup.ports[0]]).send(.text(text))
        try await Task.sleep(for: .milliseconds(300))
        // Asks the other desktop, so the clip had to cross between agents.
        let (received, _, _) = try await setup.client(ports: [setup.ports.last!]).latest()
        #expect(received?.content == .text(text))
        #expect(received?.id == sent.id)
    }

    @Test func wrongGroupIsRefused() async {
        let outsider = setup.client(key: GroupKey(bytes: Data(repeating: 9, count: 32)))
        await #expect(throws: SyncError.wrongCode) { try await outsider.send(.text("nope")) }
    }

    @Test func rememberedAddressDoesNotHideOtherDesktops() async throws {
        // Only one desktop is remembered, as after a sync the other missed.
        // The search still finds the other, and the remembered one, reached
        // both ways, counts once.
        let remembered = [Address(host: "127.0.0.1", port: setup.ports[0])]
        let (sent, addresses) = try await setup.client(remembered: remembered).send(.text("everyone \(UUID())"))
        #expect(sent.to.count == setup.ports.count)
        #expect(Set(addresses.map(\.port)) == Set(setup.ports))
    }

    @Test func latestFromARememberedAddressDoesNotWaitForTheSearch() async throws {
        _ = try await setup.client(ports: [setup.ports[0]]).send(.text("waiting \(UUID())"))
        let remembered = [Address(host: "127.0.0.1", port: setup.ports[0])]
        let client = SyncClient(group: setup.key, device: "test-phone", name: "Test iPhone", remembered: remembered, endpoints: {
            try? await Task.sleep(for: .seconds(30))
            return []
        })
        let start = ContinuousClock.now
        let (received, _, _) = try await client.latest()
        #expect(received != nil)
        #expect(ContinuousClock.now - start < .seconds(5))
    }

    @Test func staleRememberedAddressFallsBackToDiscovery() async throws {
        let stale = [Address(host: "127.0.0.1", port: 1)]
        let (sent, addresses) = try await setup.client(remembered: stale).send(.text("fallback \(UUID())"))
        #expect(sent.to.count == setup.ports.count)
        #expect(!addresses.contains(stale[0]))
    }

    @Test func stoppedComputerIsNotBlamedOnTheCode() async {
        let client = setup.client(ports: [1])
        await #expect(throws: SyncError.unreachable) { try await client.send(.text("nobody home")) }
    }

    @Test func imagesRoundTripThroughTheDesktops() async throws {
        let png = Data("\u{89}PNG pretend \(UUID())".utf8) + Data((0..<300_000).map { UInt8($0 % 251) })
        let (sent, _) = try await setup.client(ports: [setup.ports[0]]).send(.image(png))
        try await Task.sleep(for: .milliseconds(500))
        let (received, _, _) = try await setup.client(ports: [setup.ports.last!]).latest()
        #expect(received?.content == .image(png))
        #expect(received?.id == sent.id)
    }

    @Test func discoveryFindsTheAgents() async {
        let found = await Discovery.find(group: setup.key)
        #expect(found.count >= setup.ports.count)
        let other = await Discovery.find(group: GroupKey(bytes: Data(repeating: 9, count: 32)), timeout: .seconds(1))
        #expect(other.isEmpty)
    }
}
