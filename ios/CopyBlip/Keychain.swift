import Foundation
import Security

/// Stores the group key, the only secret the app has.
enum Keychain {
    private static var query: [String: Any] { [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "com.coledeterman.CopyBlip",
        kSecAttrAccount as String: "group-key",
    ] }

    static func save(_ data: Data) -> Bool {
        delete()
        var item = query
        item[kSecValueData as String] = data
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    static func load() -> Data? {
        var search = query
        search[kSecReturnData as String] = true
        var result: AnyObject?
        guard SecItemCopyMatching(search as CFDictionary, &result) == errSecSuccess else { return nil }
        return result as? Data
    }

    static func delete() {
        SecItemDelete(query as CFDictionary)
    }
}
