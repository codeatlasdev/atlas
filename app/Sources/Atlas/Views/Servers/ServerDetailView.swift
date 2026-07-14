import SwiftUI

struct ServerDetailView: View {
    @Environment(AppState.self) private var appState
    let server: Server

    @State private var services: [SystemdService] = []

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                serverInfo
                servicesSection
            }
            .padding(24)
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    Task { await appState.refreshServers() }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
            }
        }
        .task {
            await loadServices()
        }
    }

    private var serverInfo: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Connection")
                .atlasFont(.headline)
                .atlasForeground(.primary)

            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 8) {
                GridRow {
                    Text("Host")
                        .atlasForeground(.secondary)
                    Text(server.host)
                        .atlasFont(.mono)
                        .atlasForeground(.primary)
                }
                GridRow {
                    Text("User")
                        .atlasForeground(.secondary)
                    Text(server.user)
                        .atlasFont(.mono)
                        .atlasForeground(.primary)
                }
                GridRow {
                    Text("Port")
                        .atlasForeground(.secondary)
                    Text("\(server.port)")
                        .atlasFont(.mono)
                        .atlasForeground(.primary)
                }
                GridRow {
                    Text("Status")
                        .atlasForeground(.secondary)
                    Label(server.status.label, systemImage: server.status.systemImage)
                        .foregroundStyle(server.status.tintColor)
                }
            }
            .atlasFont(.body)
            .padding(12)
        }
        .cardStyle()
    }

    private var servicesSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Services")
                .atlasFont(.headline)
                .atlasForeground(.primary)

            if services.isEmpty {
                EmptyStateView(
                    icon: "gear",
                    title: "No Services",
                    description: "No systemd services found on this server."
                )
                .frame(minHeight: 150)
            } else {
                LazyVStack(spacing: 8) {
                    ForEach(services) { service in
                        ServiceRow(service: service)
                    }
                }
            }
        }
        .cardStyle()
    }

    private func loadServices() async {
        do {
            let params: [String: Any] = ["server_id": server.id.uuidString]
            let response = try await appState.daemon.send(method: "services.list", params: params)
            if let data = try? JSONSerialization.data(withJSONObject: response),
               let decoded = try? JSONDecoder.atlas.decode([SystemdService].self, from: data) {
                services = decoded
            }
        } catch {
            services = []
        }
    }
}
