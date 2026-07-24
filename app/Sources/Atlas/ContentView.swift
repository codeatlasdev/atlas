import SwiftUI

struct ContentView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        Group {
            if appState.currentProject == nil {
                WelcomeView()
            } else if appState.needsProjectSetup {
                ProjectSetupView()
            } else {
                ProjectWindow()
            }
        }
        .preferredColorScheme(.dark)
        .animation(.spring(duration: 0.3), value: appState.currentProject == nil)
        .animation(.spring(duration: 0.3), value: appState.needsProjectSetup)
        .task {
            await appState.connect()
        }
    }
}
