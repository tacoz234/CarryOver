// Spike: how does macOS treat a background process that watches and writes the
// general pasteboard? Never prints clipboard contents, only types and counts.
//
//   probe status           access behavior and change count; reads no content
//   probe write <text>     back up, write <text>, hold, restore (--hold seconds)
import AppKit

let pb = NSPasteboard.general

func behaviorName(_ b: NSPasteboard.AccessBehavior) -> String {
    switch b {
    case .default: return "default"
    case .ask: return "ask"
    case .alwaysAllow: return "alwaysAllow"
    case .alwaysDeny: return "alwaysDeny"
    @unknown default: return "unknown(\(b.rawValue))"
    }
}

func printStatus() {
    print("accessBehavior=\(behaviorName(pb.accessBehavior)) changeCount=\(pb.changeCount)")
}

func backup() -> [[NSPasteboard.PasteboardType: Data]] {
    (pb.pasteboardItems ?? []).map { item in
        var copy: [NSPasteboard.PasteboardType: Data] = [:]
        for type in item.types {
            if let data = item.data(forType: type) { copy[type] = data }
        }
        return copy
    }
}

func restore(_ saved: [[NSPasteboard.PasteboardType: Data]]) {
    pb.clearContents()
    let items = saved.map { entry -> NSPasteboardItem in
        let item = NSPasteboardItem()
        for (type, data) in entry { item.setData(data, forType: type) }
        return item
    }
    if !items.isEmpty { pb.writeObjects(items) }
}

let args = CommandLine.arguments
switch args.dropFirst().first {
case "status":
    printStatus()

case "write":
    guard args.count >= 3 else { fatalError("usage: probe write <text> [--hold seconds]") }
    let hold = args.firstIndex(of: "--hold").flatMap { Double(args[$0 + 1]) } ?? 2
    printStatus()
    let start = Date()
    let saved = backup()
    print("backup items=\(saved.count) tookSeconds=\(String(format: "%.2f", Date().timeIntervalSince(start)))")
    printStatus()

    pb.clearContents()
    let wrote = pb.setString(args[2], forType: .string)
    print("wrote=\(wrote) changeCount=\(pb.changeCount)")

    Thread.sleep(forTimeInterval: hold)
    let intact = pb.string(forType: .string) == args[2]
    print("afterHold intact=\(intact) changeCount=\(pb.changeCount)")

    restore(saved)
    let restored = backup()
    print("restored match=\(restored == saved) items=\(restored.count)")
    printStatus()

case "hold":
    // Lets another process use the real clipboard, then puts the user's contents back.
    guard args.count >= 3, let seconds = Double(args[2]) else { fatalError("usage: probe hold <seconds>") }
    let saved = backup()
    print("backed up items=\(saved.count)")
    fflush(stdout)
    Thread.sleep(forTimeInterval: seconds)
    restore(saved)
    print("restored match=\(backup() == saved)")

default:
    fatalError("usage: probe status | write <text> [--hold seconds] | hold <seconds>")
}
