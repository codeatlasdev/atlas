import SwiftUI

struct WelcomeView: View {
    @Environment(AppState.self) private var appState
    @State private var hoveredProject: String?

    var body: some View {
        ZStack {
            DS.bg.base.ignoresSafeArea()
            content
        }
        .frame(minWidth: 700, minHeight: 500)
    }

    private var content: some View {
        HStack(spacing: 0) {
            leftPanel
            SoftDivider()
                .frame(width: 0.5)
                .frame(maxHeight: .infinity)
            rightPanel
        }
    }

    // MARK: - Left Panel

    private var leftPanel: some View {
        VStack(spacing: DS.spacing.xxxl) {
            Spacer()

            // Logo
            Image(systemName: "cpu")
                .font(.system(size: 48, weight: .thin))
                .foregroundStyle(DS.accent.primary)

            // Title
            VStack(spacing: DS.spacing.sm) {
                Text("Atlas")
                    .font(.system(size: 36, weight: .bold))
                    .foregroundStyle(DS.text.primary)

                Text("Developer Platform")
                    .font(.atlasBody)
                    .foregroundStyle(DS.text.secondary)
            }

            // Actions
            VStack(spacing: DS.spacing.md) {
                AtlasButton("Open Project", icon: "folder.badge.plus") {
                    appState.openProjectPicker()
                }
                .keyboardShortcut("o", modifiers: .command)

                AtlasButton("Clone Repository", icon: "arrow.down.circle", style: .secondary) {
                    // Clone repo flow
                }
            }

            Spacer()

            Text("v0.1.0")
                .font(.atlasCaption)
                .foregroundStyle(DS.text.tertiary)
                .padding(.bottom, DS.spacing.xl)
        }
        .frame(width: 300)
        .padding(.horizontal, DS.spacing.xxxl)
    }

    // MARK: - Right Panel

    private var rightPanel: some View {
        VStack(alignment: .leading, spacing: DS.spacing.lg) {
            Text("Recent Projects")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(DS.text.secondary)
                .padding(.top, DS.spacing.xxl)
                .padding(.horizontal, DS.spacing.xxl)

            if appState.recentProjects.isEmpty {
                Spacer()
                VStack(spacing: DS.spacing.md) {
                    Image(systemName: "folder")
                        .font(.system(size: 32, weight: .light))
                        .foregroundStyle(DS.text.tertiary)
                    Text("No recent projects")
                        .font(.atlasBody)
                        .foregroundStyle(DS.text.tertiary)
                }
                .frame(maxWidth: .infinity)
                Spacer()
            } else {
                ScrollView {
                    LazyVStack(spacing: DS.spacing.xs) {
                        ForEach(appState.recentProjects) { project in
                            RecentProjectRow(
                                project: project,
                                isHovered: hoveredProject == project.id
                            ) {
                                appState.openProject(path: project.path)
                            }
                            .onHover { hovering in
                                hoveredProject = hovering ? project.id : nil
                            }
                        }
                    }
                    .padding(.horizontal, DS.spacing.lg)
                }
            }
        }
        .frame(maxWidth: .infinity)
        .background(DS.bg.elevated.opacity(0.3))
    }
}

// MARK: - Recent Project Row

struct RecentProjectRow: View {
    let project: ProjectInfo
    let isHovered: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: DS.spacing.md) {
                RoundedRectangle(cornerRadius: DS.radius.sm, style: .continuous)
                    .fill(DS.accent.subtle)
                    .frame(width: 32, height: 32)
                    .overlay {
                        Image(systemName: "folder.fill")
                            .font(.system(size: 14))
                            .foregroundStyle(DS.accent.primary)
                    }

                VStack(alignment: .leading, spacing: 2) {
                    Text(project.name)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(DS.text.primary)

                    Text(project.path)
                        .font(.system(size: 11))
                        .foregroundStyle(DS.text.tertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }

                Spacer()

                Text(project.lastOpened, style: .relative)
                    .font(.system(size: 11))
                    .foregroundStyle(DS.text.tertiary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, DS.spacing.sm)
            .background(
                RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                    .fill(isHovered ? DS.bg.hover : .clear)
            )
        }
        .buttonStyle(.plain)
    }
}
