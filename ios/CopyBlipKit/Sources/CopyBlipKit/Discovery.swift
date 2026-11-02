import Foundation
import Network

public enum Discovery {
    public static let serviceType = "_copyblip._tcp"

    /// Finds the group's devices on the local network. Returns shortly after the
    /// first one appears, or empty after `timeout`.
    public static func find(group: GroupKey, timeout: Duration = .seconds(3)) async -> [NWEndpoint] {
        let tag = group.tag
        let browser = NWBrowser(for: .bonjourWithTXTRecord(type: serviceType, domain: nil), using: .tcp)
        let found = Found()
        let results = AsyncStream<Void> { stream in
            browser.browseResultsChangedHandler = { results, _ in
                for result in results {
                    if case let .bonjour(txt) = result.metadata, txt["group"] == tag {
                        found.insert(result.endpoint)
                    }
                }
                if !found.isEmpty { stream.yield() }
            }
            browser.stateUpdateHandler = { state in
                if case .failed = state { stream.finish() }
            }
            stream.onTermination = { _ in browser.cancel() }
            browser.start(queue: DispatchQueue(label: "copyblip.discovery"))
        }

        _ = try? await withTimeout(timeout) {
            for await _ in results { return }
        }
        if !found.isEmpty {
            // A short grace period picks up other devices answering at about the same time.
            try? await Task.sleep(for: .milliseconds(300))
        }
        browser.cancel()
        return found.all
    }
}

private final class Found: @unchecked Sendable {
    private let lock = NSLock()
    private var endpoints: [NWEndpoint] = []

    func insert(_ endpoint: NWEndpoint) {
        lock.withLock { if !endpoints.contains(endpoint) { endpoints.append(endpoint) } }
    }

    var isEmpty: Bool { lock.withLock { endpoints.isEmpty } }
    var all: [NWEndpoint] { lock.withLock { endpoints } }
}
