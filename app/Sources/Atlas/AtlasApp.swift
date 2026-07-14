import SwiftUI

@main
struct AtlasApp: App {
    @State private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(appState)
        }
        .windowStyle(.titleBar)
        .defaultSize(width: 1000, height: 700)

        MenuBarExtra("Atlas", systemImage: "server.rack") {
            MenuBarView()
                .environment(appState)
        }

        Settings {
            SettingsView()
                .environment(appState)
        }
    }
}

struct MenuBarView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        VStack {
            Label(
                appState.isConnected ? "Connected" : "Disconnected",
                systemImage: appState.isConnected ? "circle.fill" : "circle"
            )
            .foregroundStyle(appState.isConnected ? .green : .secondary)

            Divider()

            Text("\(appState.servers.count) servers")

            Divider()

            Button("Quit Atlas") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q")
        }
        .padding(8)
    }
}
