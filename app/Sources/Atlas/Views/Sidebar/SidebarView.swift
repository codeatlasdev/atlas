import SwiftUI

struct SidebarView: View {
    @Environment(AppState.self) private var appState
    @Binding var selection: SidebarDestination?
    @Binding var showSpawnAgent: Bool

    var body: some View {
        List(selection: $selection) {
            // Agent Sessions
            Section {
                ForEach(appState.agentSessions) { session in
                    AgentSessionRow(session: session)
                        .tag(SidebarDestination.agent(session.id))
                }

                if appState.agentSessions.isEmpty {
                    Text("No active agents")
                        .foregroundStyle(.secondary)
                        .font(.caption)
                }
            } header: {
                HStack {
                    Label("Agents", systemImage: "cpu")
                    Spacer()
                    Button(action: { showSpawnAgent = true }) {
                        Image(systemName: "plus.circle.fill")
                            .foregroundStyle(.accent)
                    }
                    .buttonStyle(.plain)
                }
            }

            // Servers
            Section("Servers") {
                ForEach(appState.servers) { server in
                    ServerRow(server: server)
                        .tag(SidebarDestination.server(server.id))
                }

                if appState.servers.isEmpty {
                    Text("No servers")
                        .foregroundStyle(.secondary)
                        .font(.caption)
                }
            }

            // AI Chat
            Section("AI") {
                Label("Chat", systemImage: "bubble.left.and.bubble.right")
                    .tag(SidebarDestination.chat)
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("Atlas")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button(action: { showSpawnAgent = true }) {
                    Label("New Agent", systemImage: "plus")
                }
            }

            ToolbarItem(placement: .status) {
                HStack(spacing: 4) {
                    Circle()
                        .fill(appState.isConnected ? .green : .red)
                        .frame(width: 6, height: 6)
                    Text(appState.isConnected ? "Connected" : "Disconnected")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}

struct AgentSessionRow: View {
    let session: AgentSessionInfo

    var body: some View {
        HStack {
            Circle()
                .fill(statusColor)
                .frame(width: 8, height: 8)

            VStack(alignment: .leading, spacing: 2) {
                Text(session.adapter)
                    .font(.body)
                    .fontWeight(.medium)
                Text(session.activityState)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var statusColor: Color {
        switch session.activityState {
        case "Active": .green
        case "Idle": .blue
        case "WaitingInput": .orange
        case "Blocked": .red
        default: .gray
        }
    }
}
