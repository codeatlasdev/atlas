import SwiftUI

struct SidebarView: View {
    @Environment(AppState.self) private var appState
    @Binding var selection: SidebarDestination?
    @Binding var showSpawnAgent: Bool

    var body: some View {
        VStack(spacing: 0) {
            // Header
            sidebarHeader

            Divider()
                .opacity(0.4)

            // Navigation items
            ScrollView {
                VStack(spacing: 4) {
                    // Main section
                    SidebarSection(title: "Workspace") {
                        SidebarItem(
                            icon: "folder.fill",
                            label: "Project",
                            destination: .project,
                            selection: $selection
                        )
                        SidebarItem(
                            icon: "rectangle.3.group",
                            label: "Kanban",
                            destination: .kanban,
                            selection: $selection
                        )
                    }

                    SidebarSection(title: "Intelligence") {
                        SidebarItem(
                            icon: "brain.head.profile",
                            label: "Tech Lead",
                            destination: .techLead,
                            selection: $selection
                        )
                        SidebarItem(
                            icon: "cpu",
                            label: "Agents",
                            destination: .agents,
                            selection: $selection,
                            badge: appState.agentSessions.count
                        )
                    }

                    SidebarSection(title: "Infrastructure") {
                        SidebarItem(
                            icon: "server.rack",
                            label: "Servers",
                            destination: .servers,
                            selection: $selection,
                            badge: appState.servers.count
                        )
                        SidebarItem(
                            icon: "rocket",
                            label: "Deploy",
                            destination: .deploy,
                            selection: $selection
                        )
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 12)
            }

            Spacer()

            // Daemon status
            daemonStatus
        }
        .background(.ultraThinMaterial)
        .listStyle(.sidebar)
    }

    // MARK: - Header

    private var sidebarHeader: some View {
        HStack(spacing: 10) {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [AtlasColors.accentPrimary, AtlasColors.accentSecondary],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .frame(width: 32, height: 32)
                .overlay {
                    Image(systemName: "atom")
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(.white)
                }

            VStack(alignment: .leading, spacing: 1) {
                Text("Atlas")
                    .font(.system(size: 14, weight: .semibold))
                    .atlasForeground(.primary)
                Text("dev.codeatlas")
                    .font(.system(size: 10))
                    .atlasForeground(.tertiary)
            }

            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    // MARK: - Daemon Status

    private var daemonStatus: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(appState.isConnected ? AtlasColors.statusSuccess : AtlasColors.statusError)
                .frame(width: 7, height: 7)
                .shadow(
                    color: appState.isConnected
                        ? AtlasColors.statusSuccess.opacity(0.5)
                        : AtlasColors.statusError.opacity(0.5),
                    radius: 3
                )

            Text(appState.isConnected ? "Daemon connected" : "Disconnected")
                .atlasFont(.caption)
                .atlasForeground(.secondary)

            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background {
            Rectangle()
                .fill(AtlasColors.backgroundSurface.opacity(0.3))
        }
        .overlay(alignment: .top) {
            Divider().opacity(0.4)
        }
    }
}

// MARK: - Sidebar Section

struct SidebarSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title.uppercased())
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(AtlasColors.textTertiary)
                .padding(.horizontal, 8)
                .padding(.top, 12)
                .padding(.bottom, 4)

            content()
        }
    }
}

// MARK: - Sidebar Item

struct SidebarItem: View {
    let icon: String
    let label: String
    let destination: SidebarDestination
    @Binding var selection: SidebarDestination?
    var badge: Int = 0

    @State private var isHovered = false

    private var isSelected: Bool {
        selection == destination
    }

    var body: some View {
        Button {
            withAnimation(.spring(response: 0.3, dampingFraction: 0.8)) {
                selection = destination
            }
        } label: {
            HStack(spacing: 10) {
                Image(systemName: icon)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(isSelected ? AtlasColors.accentPrimary : AtlasColors.textSecondary)
                    .frame(width: 20)

                Text(label)
                    .font(.system(size: 13, weight: isSelected ? .medium : .regular))
                    .foregroundStyle(isSelected ? AtlasColors.textPrimary : AtlasColors.textSecondary)

                Spacer()

                if badge > 0 {
                    CountBadge(
                        count: badge,
                        color: isSelected ? AtlasColors.accentPrimary : AtlasColors.textTertiary
                    )
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(
                        isSelected
                            ? AtlasColors.sidebarSelected
                            : (isHovered ? AtlasColors.sidebarHover : .clear)
                    )
            }
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            withAnimation(.easeInOut(duration: 0.15)) {
                isHovered = hovering
            }
        }
    }
}

// MARK: - Agent Session Row (for inline display)

struct AgentSessionRow: View {
    let session: AgentSessionInfo

    var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(statusColor)
                .frame(width: 7, height: 7)

            VStack(alignment: .leading, spacing: 1) {
                Text(session.adapter)
                    .font(.system(size: 12, weight: .medium))
                    .atlasForeground(.primary)
                Text(session.activityState)
                    .font(.system(size: 10))
                    .atlasForeground(.tertiary)
            }
        }
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
