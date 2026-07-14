import SwiftUI

struct ContentView: View {
    @Environment(AppState.self) private var appState
    @State private var selectedDestination: SidebarDestination?

    var body: some View {
        NavigationSplitView {
            SidebarView(selection: $selectedDestination)
        } detail: {
            detailView
        }
        .task {
            await appState.connect()
        }
    }

    @ViewBuilder
    private var detailView: some View {
        switch selectedDestination {
        case .server(let id):
            if let server = appState.servers.first(where: { $0.id == id }) {
                ServerDetailView(server: server)
            } else {
                ContentUnavailableView("Server not found", systemImage: "server.rack")
            }
        case .chat:
            ChatView()
        case nil:
            ContentUnavailableView(
                "Select an item",
                systemImage: "sidebar.left",
                description: Text("Choose a server or start an AI chat session")
            )
        }
    }
}

enum SidebarDestination: Hashable {
    case server(UUID)
    case chat
}
