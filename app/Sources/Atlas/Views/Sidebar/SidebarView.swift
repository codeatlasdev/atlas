import SwiftUI

// MARK: - Legacy Sidebar Destination (used by older navigation)

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

// MARK: - Server List View (used by ProjectWindow)

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
        .background(AtlasColors.backgroundDeep)
    }
}

struct ServerCardView: View {
    let server: Server

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "server.rack")
                .font(.system(size: 18))
                .foregroundStyle(AtlasColors.neonCyan)

            VStack(alignment: .leading, spacing: 2) {
                Text(server.name)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(AtlasColors.textPrimary)
                Text(server.host)
                    .font(.system(size: 12))
                    .foregroundStyle(AtlasColors.textSecondary)
            }

            Spacer()

            StatusBadge(
                label: server.status.label,
                color: server.status == .online ? AtlasColors.neonGreen : AtlasColors.neonRed
            )
        }
        .cardStyle()
    }
}

// MARK: - Agent List View (used by ProjectWindow)

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
        .background(AtlasColors.backgroundDeep)
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
                .shadow(color: statusColor.opacity(0.5), radius: 3)

            VStack(alignment: .leading, spacing: 2) {
                Text(session.adapter)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(AtlasColors.textPrimary)
                Text(session.activityState)
                    .font(.system(size: 12))
                    .foregroundStyle(AtlasColors.textSecondary)
            }

            Spacer()

            StatusBadge(label: session.activityState, color: statusColor)
        }
        .cardStyle()
    }

    private var statusColor: Color {
        switch session.activityState {
        case "Active": AtlasColors.neonGreen
        case "Idle": AtlasColors.neonCyan
        case "WaitingInput": AtlasColors.neonAmber
        case "Blocked": AtlasColors.neonRed
        default: AtlasColors.textTertiary
        }
    }
}
