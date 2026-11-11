# iPhone and iPad distribution

The app is version **0.4.0 (3)**, bundle ID `com.coledeterman.CopyBlip`, team `A6GZ8F6PF5`. The intended release is **free**, with **manual release after approval**. Do not submit for App Review as part of build or metadata preparation.

## Assets

- `metadata.en-US.json`: store copy, URLs, privacy answers and intended settings. This is a preparation file, not a raw App Store Connect API request.
- `review-notes.txt`: steps to review the real local-network workflow with the free computer companion. The app has no account or demo login.
- `screenshots/en-US`: real simulator captures using synthetic clipboard items and an isolated test group; no production pairing keys or personal clipboard contents. iPhone: 1320 × 2868; iPad: 2064 × 2752. These are PNG app screenshots, not generated mockups.
- `ExportOptions.plist`: automatic App Store distribution signing. Private signing keys and API credentials must stay outside the repository.

Public URLs:

- Privacy: https://tacoz234.github.io/CarryOver/privacy.html
- Support: https://tacoz234.github.io/CarryOver/support.html
- Marketing: https://tacoz234.github.io/CarryOver/
- Public support/privacy contact: determantechhelp@gmail.com

## Build and upload

With full Xcode installed and the existing developer account signed in:

```sh
ios/scripts/prepare-app-store.sh
```

This archives Release and exports `ios/build/app-store/export/CopyBlip.ipa`. The build output is ignored by Git. The script sets `DEVELOPER_DIR` only for its process if needed; it does not change the global Xcode selection.

An App Store Connect app record must already exist before uploading. Create it in the website with iOS, English (U.S.), bundle ID `com.coledeterman.CopyBlip`, and SKU `carryover-ios`. Apple's public API cannot create the initial app record. Its name must be available in the account.

Once the record exists:

```sh
ios/scripts/prepare-app-store.sh --upload
```

Uploading does not submit the app for App Review, start external beta review, or release it. Check Apple's processing result before associating the build with a store version. Increase `CURRENT_PROJECT_VERSION` in both `project.yml` and `project.pbxproj` before uploading a different binary under a previously uploaded build number.

## Privacy and encryption

The iOS app uses Apple CryptoKit for ChaChaPoly, HKDF and SHA256 and links no third-party crypto implementation. `ITSAppUsesNonExemptEncryption = NO` declares exemption from encryption documentation; it does not mean there is no encryption. Reassess if dependencies, algorithms or distribution change. Apple's guidance: https://developer.apple.com/documentation/security/complying-with-encryption-export-regulations .

The source privacy manifest declares no tracking or collected data, and UserDefaults reason CA92.1 for the app's own settings. In the App Privacy questionnaire, the prepared answer is **Data Not Collected**: clips, history and peer identity remain on user-paired devices, with no developer-operated backend or analytics. User-initiated support email and GitHub hosting are disclosed separately in the public policy. The privacy manifest does not publish the App Store privacy label by itself.

## Before review (a separate step)

Verify the saved store record, processed build, price and manual-release choice. Finish the current age-rating questionnaire, App Privacy publication and any account-specific agreements or regional declarations in App Store Connect. Do not guess trader status or accept agreements automatically. Add a real review contact name and phone, then use the prepared reviewer notes. The Mac companion is not notarized yet; those notes disclose its installation flow.

Run a TestFlight smoke test of fresh setup, denied camera/local-network/paste permissions, text/images in both directions, reconnection after a computer restarts, and history deletion on iPhone and iPad. No public accessibility-support claims should be made until separately tested.
