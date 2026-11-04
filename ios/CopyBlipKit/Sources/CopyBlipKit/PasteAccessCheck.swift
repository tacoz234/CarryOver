/// A result from an explicit paste-read test. It never represents iOS's
/// persistent Ask/Allow preference, which isn't exposed by a public API.
public enum PasteAccessCheck: Equatable, Sendable {
    case notChecked, passed, denied, empty, ownCopy

    public static func test(hasText: Bool, cameFromCarryover: Bool, canReadText: () -> Bool) -> Self {
        guard hasText else { return .empty }
        guard !cameFromCarryover else { return .ownCopy }
        return canReadText() ? .passed : .denied
    }
}
