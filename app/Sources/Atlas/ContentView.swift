import SwiftUI

struct ContentView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        Group {
            if appState.currentProject == nil {
                WelcomeView()
            } else {
                ProjectWindow()
            }
        }
        .preferredColorScheme(.dark)
        .animation(.spring(duration: 0.3), value: appState.currentProject == nil)
        .task {
            await appState.connect()
        }
    }
}
