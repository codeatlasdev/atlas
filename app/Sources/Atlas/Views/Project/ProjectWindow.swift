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
                .background(DS.bg.base)
        }
        .navigationSplitViewStyle(.balanced)
        .background(DS.bg.base)
        .sheet(isPresented: $showSpawnAgent) {
            SpawnAgentView()
        }
    }

    // MARK: - Sidebar

    private var projectSidebar: some View {
        VStack(spacing: 0) {
            projectHeader

            SoftDivider()

            ScrollView {
                VStack(spacing: 2) {
                    ForEach(SidebarTab.allCases, id: \.self) { tab in
                        ProjectSidebarItem(
                            tab: tab,
                            isSelected: appState.selectedTab == tab,
                            badge: badgeCount(for: tab)
                        ) {
                            withAnimation(.easeInOut(duration: 0.2)) {
                                appState.selectedTab = tab
                            }
                        }
                    }
                }
                .padding(.horizontal, DS.spacing.md)
                .padding(.top, DS.spacing.md)
            }

            Spacer()

            daemonStatusBar
        }
        .background(.ultraThinMaterial)
    }

    private var projectHeader: some View {
        HStack(spacing: 10) {
            RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                .fill(DS.accent.subtle)
                .frame(width: 30, height: 30)
                .overlay {
                    Image(systemName: "folder.fill")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(DS.accent.primary)
                }

            VStack(alignment: .leading, spacing: 1) {
                Text(appState.currentProject?.name ?? "Project")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(DS.text.primary)
                    .lineLimit(1)

                Text("atlas workspace")
                    .font(.system(size: 10))
                    .foregroundStyle(DS.text.tertiary)
            }

            Spacer()

            Button {
                appState.closeProject()
            } label: {
                Image(systemName: "xmark.circle")
                    .font(.system(size: 14))
                    .foregroundStyle(DS.text.tertiary)
            }
            .buttonStyle(.plain)
            .help("Close Project")
        }
        .padding(.horizontal, 14)
        .padding(.vertical, DS.spacing.md)
    }

    private var daemonStatusBar: some View {
        HStack(spacing: DS.spacing.sm) {
            Circle()
                .fill(appState.isConnected ? DS.status.success : DS.status.error)
                .frame(width: 7, height: 7)

            Text(appState.isConnected ? "Daemon connected" : "Disconnected")
                .font(.system(size: 11))
                .foregroundStyle(DS.text.secondary)

            Spacer()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(DS.bg.elevated.opacity(0.5))
        .overlay(alignment: .top) {
            SoftDivider()
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
                icon: "paperplane.fill",
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

// MARK: - Sidebar Item

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
                    .foregroundStyle(isSelected ? DS.accent.primary : DS.text.secondary)
                    .frame(width: 20)

                Text(tab.rawValue)
                    .font(.system(size: 13, weight: isSelected ? .medium : .regular))
                    .foregroundStyle(isSelected ? DS.text.primary : DS.text.secondary)

                Spacer()

                if badge > 0 {
                    CountBadge(
                        count: badge,
                        color: isSelected ? DS.accent.primary : DS.text.tertiary
                    )
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                    .fill(
                        isSelected
                            ? DS.accent.subtle
                            : (isHovered ? DS.bg.hover : .clear)
                    )
            )
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            withAnimation(.easeInOut(duration: 0.15)) {
                isHovered = hovering
            }
        }
    }
}
