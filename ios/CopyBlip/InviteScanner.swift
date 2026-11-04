import AVFoundation
import CopyBlipKit
import SwiftUI
import VisionKit

/// Camera access is requested only when the user chooses Scan QR Code.
struct InviteScanner: View {
    let scanned: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @Environment(\.scenePhase) private var scenePhase
    @State private var allowed = false
    @State private var checked = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            VStack(spacing: 20) {
                if allowed && DataScannerViewController.isSupported && DataScannerViewController.isAvailable {
                    ScannerCamera(scanned: scanned, failed: { error = $0 })
                        .clipShape(RoundedRectangle(cornerRadius: 18))
                        .overlay(alignment: .bottom) {
                            Text("Point at the QR code in Carryover → Add a Device.")
                                .font(.callout).padding().background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12)).padding()
                        }
                        .accessibilityLabel("Camera scanning for a Carryover QR code")
                } else if checked {
                    ContentUnavailableView("Camera Unavailable", systemImage: "camera", description: Text(allowed
                        ? "QR scanning isn't available on this device right now. You can paste an invite link instead."
                        : "Allow camera access in Settings to scan a QR code, or go back and paste an invite link."))
                    if !allowed {
                        Button("Open Settings") { openURL(URL(string: UIApplication.openSettingsURLString)!) }
                    }
                } else {
                    ProgressView("Opening camera…")
                }
                if let error { Text(error).font(.callout).foregroundStyle(.red) }
                Button("Use Invite Link Instead") { dismiss() }.buttonStyle(.bordered)
            }
            .padding()
            .navigationTitle("Scan QR Code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .task {
                guard DataScannerViewController.isSupported else { checked = true; return }
                let status = AVCaptureDevice.authorizationStatus(for: .video)
                if status == .notDetermined {
                    allowed = await AVCaptureDevice.requestAccess(for: .video)
                } else {
                    allowed = status == .authorized
                }
                checked = true
            }
            .onChange(of: scenePhase) { _, phase in
                if phase == .active {
                    allowed = AVCaptureDevice.authorizationStatus(for: .video) == .authorized
                }
            }
        }
    }
}

private struct ScannerCamera: UIViewControllerRepresentable {
    let scanned: (String) -> Void
    let failed: (String) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(scanned: scanned, failed: failed) }

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .balanced,
            recognizesMultipleItems: false,
            isHighFrameRateTrackingEnabled: false,
            isPinchToZoomEnabled: true,
            isGuidanceEnabled: true,
            isHighlightingEnabled: true
        )
        scanner.delegate = context.coordinator
        do { try scanner.startScanning() } catch {
            DispatchQueue.main.async { failed("Couldn't start the camera. Close the scanner and try again, or paste your invite link.") }
        }
        return scanner
    }

    func updateUIViewController(_ controller: DataScannerViewController, context: Context) {}

    static func dismantleUIViewController(_ controller: DataScannerViewController, coordinator: Coordinator) {
        controller.stopScanning()
    }

    @MainActor final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let scanned: (String) -> Void
        let failed: (String) -> Void
        private var delivered = false

        init(scanned: @escaping (String) -> Void, failed: @escaping (String) -> Void) {
            self.scanned = scanned
            self.failed = failed
        }

        func dataScanner(_ scanner: DataScannerViewController, didAdd addedItems: [RecognizedItem], allItems: [RecognizedItem]) {
            guard !delivered else { return }
            for item in addedItems {
                guard case let .barcode(barcode) = item, let value = barcode.payloadStringValue else { continue }
                guard Invite(value) != nil else {
                    failed("That QR code isn't a Carryover invite. Scan the code in Add a Device on your other device.")
                    continue
                }
                delivered = true
                scanner.stopScanning()
                scanned(value)
                return
            }
        }

        func dataScanner(_ scanner: DataScannerViewController, becameUnavailableWithError error: DataScannerViewController.ScanningUnavailable) {
            scanner.stopScanning()
            failed("The camera is unavailable. Close the scanner and try again, or paste your invite link.")
        }
    }
}
