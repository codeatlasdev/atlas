import SwiftUI

struct ContentView: View {
    @Environment(AppState.self) private var appState
    @State private var selectedDestination: SidebarDestination?
    @State private var showSpawnAgent = false

    var body: some View {
        NavigationSplitView {
            SidebarView(selection: $selectedDestination, showSpawnAgent: $showSpawnAgent)
        } detail: {
            detailView
        }
        .task {
            await appState.connect()
        }
        .sheet(isPresented: $showSpawnAgent) {
            SpawnAgentView()
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
        case .agent(let id):
            if let session = appState.agentSessions.first(where: { $0.id == id }) {
                AgentSessionView(session: session)
            } else {
                ContentUnavailableView("Session not found", systemImage: "cpu")
            }
        case .chat:
            ChatView()
        case nil:
            ContentUnavailableView(
                "Atlas",
                systemImage: "cpu",
                description: Text("Select an agent session, server, or start a chat")
            )
        }
    }
}

enum SidebarDestination: Hashable {
    case server(UUID)
    case agent(String)
    case chat
}
