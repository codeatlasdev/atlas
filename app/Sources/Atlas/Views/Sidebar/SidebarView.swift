import SwiftUI

struct SidebarView: View {
    @Environment(AppState.self) private var appState
    @Binding var selection: SidebarDestination?

    var body: some View {
        List(selection: $selection) {
            Section("Servers") {
                ForEach(appState.servers) { server in
                    ServerRow(server: server)
                        .tag(SidebarDestination.server(server.id))
                }
            }

            Section("AI") {
                Label("Chat", systemImage: "bubble.left.and.bubble.right")
                    .tag(SidebarDestination.chat)
            }

            if !appState.sessions.filter({ $0.isActive }).isEmpty {
                Section("Active Sessions") {
                    ForEach(appState.sessions.filter { $0.isActive }) { session in
                        Label(session.kind.label, systemImage: session.kind.systemImage)
                            .badge(session.durationLabel)
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) {
            connectionStatus
                .padding(12)
        }
    }

    private var connectionStatus: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(appState.isConnected ? .green : .red)
                .frame(width: 8, height: 8)
            Text(appState.isConnected ? "Daemon connected" : "Disconnected")
                .atlasFont(.caption)
                .foregroundStyle(.textSecondary)
            Spacer()
        }
    }
}
