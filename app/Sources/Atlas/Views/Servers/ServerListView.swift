import SwiftUI

struct ServerListView: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        List(appState.servers) { server in
            NavigationLink(value: SidebarDestination.server(server.id)) {
                ServerRow(server: server)
            }
        }
        .navigationTitle("Servers")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    // Add server action
                } label: {
                    Label("Add Server", systemImage: "plus")
                }
            }
        }
    }
}
