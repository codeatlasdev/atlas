import SwiftUI

struct ProjectWindow: View {
    @Environment(AppState.self) private var appState
    @State private var showSpawnAgent = false

    var body: some View {
        @Bindable var state = appState

        NavigationSplitView {
            projectSidebar
                .navigationSplitViewColumnWidth(min: 200, ideal: 220, max: 260)
        } detail: {
            detailContent
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(AtlasColors.backgroundDeep)
        }
        .navigationSplitViewStyle(.balanced)
        .background(AtlasColors.backgroundDeep)
        .sheet(isPresented: $showSpawnAgent) {
            SpawnAgentView()
        }
    }

    // MARK: - Sidebar

    private var projectSidebar: some View {
        VStack(spacing: 0) {
            // Project header
            projectHeader

            Divider().background(AtlasColors.border)

            // Tabs
            ScrollView {
                VStack(spacing: 2) {
                    ForEach(SidebarTab.allCases, id: \.self) { tab in
                        ProjectSidebarItem(
                            tab: tab,
                            isSelected: appState.selectedTab == tab,
                            badge: badgeCount(for: tab)
                        ) {
                            withAnimation(.spring(duration: 0.3)) {
                                appState.selectedTab = tab
                            }
                        }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 12)
            }

            Spacer()

            // Daemon status
            daemonStatusBar
        }
        .background(AtlasColors.backgroundSurface)
    }

    private var projectHeader: some View {
        HStack(spacing: 10) {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [AtlasColors.neonCyan, AtlasColors.neonPurple],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .frame(width: 30, height: 30)
                .overlay {
                    Image(systemName: "folder.fill")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(.white)
                }

            VStack(alignment: .leading, spacing: 1) {
                Text(appState.currentProject?.name ?? "Project")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(AtlasColors.textPrimary)
                    .lineLimit(1)

                Text("atlas workspace")
                    .font(.system(size: 10))
                    .foregroundStyle(AtlasColors.textTertiary)
            }

            Spacer()

            Button {
                appState.closeProject()
            } label: {
                Image(systemName: "xmark.circle")
                    .font(.system(size: 14))
                    .foregroundStyle(AtlasColors.textTertiary)
            }
            .buttonStyle(.plain)
            .help("Close Project")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
    }

    private var daemonStatusBar: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(appState.isConnected ? AtlasColors.neonGreen : AtlasColors.neonRed)
                .frame(width: 7, height: 7)
                .shadow(
                    color: (appState.isConnected ? AtlasColors.neonGreen : AtlasColors.neonRed).opacity(0.5),
                    radius: 3
                )

            Text(appState.isConnected ? "Daemon connected" : "Disconnected")
                .font(.system(size: 11))
                .foregroundStyle(AtlasColors.textSecondary)

            Spacer()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(AtlasColors.backgroundElevated.opacity(0.5))
        .overlay(alignment: .top) {
            Rectangle()
                .fill(AtlasColors.border)
                .frame(height: 0.5)
        }
    }

    // MARK: - Detail Content

    @ViewBuilder
    private var detailContent: some View {
        switch appState.selectedTab {
        case .kanban:
            KanbanView()
        case .techLead:
            TechLeadChatView()
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
        case .deploy:
            EmptyStateView(
                icon: "rocket",
                title: "Deploy",
                description: "Deploy your services to production with one click."
            )
        }
    }

    private func badgeCount(for tab: SidebarTab) -> Int {
        switch tab {
        case .agents: appState.agentSessions.count
        case .servers: appState.servers.count
        case .kanban: appState.tasks.filter { $0.status == .inProgress }.count
        default: 0
        }
    }
}

// MARK: - Project Sidebar Item

struct ProjectSidebarItem: View {
    let tab: SidebarTab
    let isSelected: Bool
    var badge: Int = 0
    let action: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: tab.icon)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(isSelected ? AtlasColors.neonCyan : AtlasColors.textSecondary)
                    .frame(width: 20)

                Text(tab.rawValue)
                    .font(.system(size: 13, weight: isSelected ? .medium : .regular))
                    .foregroundStyle(isSelected ? AtlasColors.textPrimary : AtlasColors.textSecondary)

                Spacer()

                if badge > 0 {
                    CountBadge(
                        count: badge,
                        color: isSelected ? AtlasColors.neonCyan : AtlasColors.textTertiary
                    )
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(
                        isSelected
                            ? AtlasColors.neonCyan.opacity(0.1)
                            : (isHovered ? AtlasColors.sidebarHover : .clear)
                    )
            }
            .overlay {
                if isSelected {
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .strokeBorder(AtlasColors.neonCyan.opacity(0.2), lineWidth: 0.5)
                }
            }
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            withAnimation(.easeInOut(duration: 0.15)) {
                isHovered = hovering
            }
        }
    }
}
