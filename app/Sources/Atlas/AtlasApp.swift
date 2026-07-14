import SwiftUI

@main
struct AtlasApp: App {
    @State private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(appState)
                .frame(minWidth: 900, minHeight: 600)
                .background(AtlasColors.backgroundDeep)
        }
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unified(showsTitle: false))
        .defaultSize(width: 1200, height: 780)

        MenuBarExtra("Atlas", systemImage: "atom") {
            MenuBarView()
                .environment(appState)
        }

        Settings {
            SettingsView()
                .environment(appState)
                .preferredColorScheme(.dark)
        }
    }
}

struct MenuBarView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 6) {
                Circle()
                    .fill(appState.isConnected ? AtlasColors.neonGreen : AtlasColors.neonRed)
                    .frame(width: 7, height: 7)
                Text(appState.isConnected ? "Daemon Connected" : "Disconnected")
                    .font(.system(size: 12, weight: .medium))
            }
            .padding(.vertical, 4)

            Divider()

            if let project = appState.currentProject {
                Label(project.name, systemImage: "folder.fill")
                    .font(.system(size: 12))
                Divider()
            }

            Label("\(appState.servers.count) Servers", systemImage: "server.rack")
                .font(.system(size: 12))

            Label("\(appState.agentSessions.count) Agents", systemImage: "cpu")
                .font(.system(size: 12))

            Divider()

            Button("Quit Atlas") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q")
        }
        .padding(10)
    }
}
