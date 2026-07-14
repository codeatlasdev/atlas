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
            .padding(20)
        }
        .navigationTitle(server.name)
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
        GroupBox("Connection") {
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 8) {
                GridRow {
                    Text("Host").foregroundStyle(.textSecondary)
                    Text(server.host).atlasFont(.mono)
                }
                GridRow {
                    Text("User").foregroundStyle(.textSecondary)
                    Text(server.user).atlasFont(.mono)
                }
                GridRow {
                    Text("Port").foregroundStyle(.textSecondary)
                    Text("\(server.port)").atlasFont(.mono)
                }
                GridRow {
                    Text("Status").foregroundStyle(.textSecondary)
                    Label(server.status.label, systemImage: server.status.systemImage)
                        .foregroundStyle(server.status.tint)
                }
            }
            .padding(8)
        }
    }

    private var servicesSection: some View {
        GroupBox("Services") {
            if services.isEmpty {
                ContentUnavailableView(
                    "No services",
                    systemImage: "gear",
                    description: Text("No systemd services found")
                )
                .frame(maxWidth: .infinity, minHeight: 100)
            } else {
                LazyVStack(spacing: 4) {
                    ForEach(services) { service in
                        ServiceRow(service: service)
                    }
                }
                .padding(4)
            }
        }
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
