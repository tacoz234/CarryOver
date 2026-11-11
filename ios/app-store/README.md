# iPhone and iPad distribution

The app is version **0.4.0 (3)**, bundle ID `com.coledeterman.CopyBlip`, team `A6GZ8F6PF5`. The intended release is **free**, with **manual release after approval**. Do not submit for App Review as part of build or metadata preparation.

## App Store Connect preparation status

App record: https://appstoreconnect.apple.com/apps/6833283899/distribution/ios/version/inflight

On November 10, 2026, version 0.4.0 build 3 was uploaded, processed successfully and selected for the store version. Internal TestFlight testing is ready. The live listing has the en-US copy, support/marketing/privacy URLs, Utilities and Productivity categories, 4+ age rating, a free price (USA base with automatic regional equalization), availability selected in 175 territories, and manual release. No App Review or external beta review submission was created.

The privacy URL and the App Privacy data-collection label are separate settings. The latter still needs to be published as **Data Not Collected** on Apple's website; API preparation does not publish it. Reviewer instructions remain in `review-notes.txt` until a private contact first name, last name and phone number are provided to Apple. Account-specific declarations or agreements must be checked by the account holder before review.

**Build 3 is already uploaded.** Bump the build number before uploading another binary. Do not rerun upload expecting Apple to replace that build.

## Assets

- `metadata.en-US.json`: store copy, URLs, privacy answers and intended settings. This is a preparation file, not a raw App Store Connect API request.
- `review-notes.txt`: steps to review the real local-network workflow with the free computer companion. The app has no account or demo login.
- `screenshots/en-US`: real simulator captures using synthetic clipboard items and an isolated test group; no production pairing keys or personal clipboard contents. Required medium iPhone: 1206 × 2622; additional large iPhone: 1320 × 2868; iPad: 2064 × 2752. These are PNG app screenshots, not generated mockups.
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

The App Store Connect record is **Carryover: Shared Clipboard**, app ID `6833283899`, primary language English (U.S.), bundle ID `com.coledeterman.CopyBlip`, SKU `carryover-ios`. The installed app continues to display Carryover. Apple's public API cannot create the initial app record; it was created on the website.

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
