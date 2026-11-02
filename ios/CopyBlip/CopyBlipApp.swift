import SwiftUI

@main
struct CopyBlipApp: App {
    @State private var model = SyncModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            ContentView(model: model)
                .onChange(of: scenePhase, initial: true) { _, phase in
                    switch phase {
                    case .active: model.appOpened()
                    case .background: model.appClosed()
                    default: break
                    }
                }
        }
    }
}
