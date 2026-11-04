@testable import CopyBlipKit
import Testing

@Test func pasteCheckDoesNotReadAnEmptyOrAppOwnedClipboard() {
    var reads = 0
    let read = { reads += 1; return true }
    #expect(PasteAccessCheck.test(hasText: false, cameFromCarryover: false, canReadText: read) == .empty)
    #expect(PasteAccessCheck.test(hasText: true, cameFromCarryover: true, canReadText: read) == .ownCopy)
    #expect(reads == 0)
}

@Test func pasteCheckMarksOnlyASuccessfulExternalReadAsPassed() {
    var reads = 0
    #expect(PasteAccessCheck.test(hasText: true, cameFromCarryover: false) { reads += 1; return true } == .passed)
    #expect(PasteAccessCheck.test(hasText: true, cameFromCarryover: false) { reads += 1; return false } == .denied)
    #expect(reads == 2)
}
