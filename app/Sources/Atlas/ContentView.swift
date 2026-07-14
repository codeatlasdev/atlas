import SwiftUI

struct ContentView: View {
    @Environment(AppState.self) private var appState
    @State private var selectedDestination: SidebarDestination? = .project
    @State private var showSpawnAgent = false

    var body: some View {
        NavigationSplitView {
            SidebarView(selection: $selectedDestination, showSpawnAgent: $showSpawnAgent)
                .navigationSplitViewColumnWidth(min: 220, ideal: 240, max: 280)
        } detail: {
            detailView
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .atlasBackground(.base)
        }
        .navigationSplitViewStyle(.balanced)
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
        case .project:
            EmptyStateView(
                icon: "folder.fill",
                title: "Project",
                description: "Configure your project settings, atlas.yaml, and team preferences."
            )

        case .kanban:
            EmptyStateView(
                icon: "rectangle.3.group",
                title: "Kanban Board",
                description: "Track tasks, sprints, and progress across your team."
            )

        case .techLead:
            ChatView()

        case .agents:
            if appState.agentSessions.isEmpty {
                EmptyStateView(
                    icon: "cpu",
                    title: "No Active Agents",
                    description: "Spawn an AI agent to start coding autonomously.",
                    actionLabel: "New Agent",
                    action: { showSpawnAgent = true }
                )
            } else {
                AgentListView(showSpawnAgent: $showSpawnAgent)
            }

        case .agentDetail(let id):
            if let session = appState.agentSessions.first(where: { $0.id == id }) {
                AgentSessionView(session: session)
            } else {
                EmptyStateView(
                    icon: "cpu",
                    title: "Session Not Found",
                    description: "This agent session is no longer available."
                )
            }

        case .servers:
            if appState.servers.isEmpty {
                EmptyStateView(
                    icon: "server.rack",
                    title: "No Servers",
                    description: "Add a server to manage deployments and services.",
                    actionLabel: "Add Server",
                    action: {}
                )
            } else {
                ServerListView()
            }

        case .serverDetail(let id):
            if let server = appState.servers.first(where: { $0.id == id }) {
                ServerDetailView(server: server)
            } else {
                EmptyStateView(
                    icon: "server.rack",
                    title: "Server Not Found",
                    description: "This server is no longer available."
                )
            }

        case .deploy:
            EmptyStateView(
                icon: "rocket",
                title: "Deploy",
                description: "Deploy your services to production with one click."
            )

        case nil:
            EmptyStateView(
                icon: "cpu",
                title: "Atlas",
                description: "Select a section from the sidebar to get started."
            )
        }
    }
}

// MARK: - Navigation Destinations

enum SidebarDestination: Hashable {
    case project
    case kanban
    case techLead
    case agents
    case agentDetail(String)
    case servers
    case serverDetail(UUID)
    case deploy
}

// MARK: - Placeholder Views

struct AgentListView: View {
    @Environment(AppState.self) private var appState
    @Binding var showSpawnAgent: Bool

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 12) {
                ForEach(appState.agentSessions) { session in
                    AgentCardView(session: session)
                }
            }
            .padding(24)
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button(action: { showSpawnAgent = true }) {
                    Label("New Agent", systemImage: "plus")
                }
            }
        }
    }
}

struct AgentCardView: View {
    let session: AgentSessionInfo

    var body: some View {
        HStack(spacing: 12) {
            Circle()
                .fill(statusColor)
                .frame(width: 10, height: 10)

            VStack(alignment: .leading, spacing: 2) {
                Text(session.adapter)
                    .atlasFont(.headline)
                    .atlasForeground(.primary)
                Text(session.activityState)
                    .atlasFont(.caption)
                    .atlasForeground(.secondary)
            }

            Spacer()

            StatusBadge(label: session.activityState, color: statusColor)
        }
        .cardStyle()
    }

    private var statusColor: Color {
        switch session.activityState {
        case "Active": AtlasColors.statusSuccess
        case "Idle": AtlasColors.statusInfo
        case "WaitingInput": AtlasColors.statusWarning
        case "Blocked": AtlasColors.statusError
        default: AtlasColors.textTertiary
        }
    }
}

struct ServerListView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 12) {
                ForEach(appState.servers) { server in
                    ServerCardView(server: server)
                }
            }
            .padding(24)
        }
    }
}

struct ServerCardView: View {
    let server: Server

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "server.rack")
                .font(.system(size: 18))
                .foregroundStyle(AtlasColors.accentSecondary)

            VStack(alignment: .leading, spacing: 2) {
                Text(server.name)
                    .atlasFont(.headline)
                    .atlasForeground(.primary)
                Text(server.host)
                    .atlasFont(.caption)
                    .atlasForeground(.secondary)
            }

            Spacer()

            StatusBadge(
                label: server.status.label,
                color: server.status == .online ? AtlasColors.statusSuccess : AtlasColors.statusError
            )
        }
        .cardStyle()
    }
}
