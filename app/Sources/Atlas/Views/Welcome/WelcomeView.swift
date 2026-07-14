import SwiftUI

struct WelcomeView: View {
    @Environment(AppState.self) private var appState
    @State private var logoGlow = false

    var body: some View {
        ZStack {
            backgroundGradient
            content
        }
        .frame(minWidth: 700, minHeight: 500)
        .onAppear {
            withAnimation(.easeInOut(duration: 2).repeatForever(autoreverses: true)) {
                logoGlow = true
            }
        }
    }

    private var backgroundGradient: some View {
        ZStack {
            AtlasColors.backgroundDeep
            RadialGradient(
                colors: [AtlasColors.neonPurple.opacity(0.08), .clear],
                center: .top,
                startRadius: 0,
                endRadius: 600
            )
            RadialGradient(
                colors: [AtlasColors.neonCyan.opacity(0.05), .clear],
                center: .bottomLeading,
                startRadius: 0,
                endRadius: 400
            )
        }
        .ignoresSafeArea()
    }

    private var content: some View {
        HStack(spacing: 0) {
            leftPanel
            Divider().background(AtlasColors.border)
            rightPanel
        }
    }

    // MARK: - Left Panel (Branding + Actions)

    private var leftPanel: some View {
        VStack(spacing: 32) {
            Spacer()

            // Logo
            ZStack {
                Circle()
                    .fill(
                        RadialGradient(
                            colors: [AtlasColors.neonCyan.opacity(0.2), .clear],
                            center: .center,
                            startRadius: 20,
                            endRadius: logoGlow ? 60 : 40
                        )
                    )
                    .frame(width: 120, height: 120)

                Image(systemName: "atom")
                    .font(.system(size: 52, weight: .thin))
                    .foregroundStyle(AtlasColors.gradientPrimary)
                    .shadow(color: AtlasColors.neonCyan.opacity(0.5), radius: logoGlow ? 15 : 8)
            }

            // Title
            VStack(spacing: 8) {
                Text("Atlas")
                    .font(.system(size: 42, weight: .bold, design: .default))
                    .foregroundStyle(AtlasColors.gradientPrimary)

                Text("Developer Platform")
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(AtlasColors.textSecondary)
            }

            // Action Buttons
            VStack(spacing: 12) {
                Button {
                    appState.openProjectPicker()
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "folder.badge.plus")
                        Text("Open Project")
                    }
                    .frame(width: 200)
                }
                .buttonStyle(GradientButtonStyle())
                .keyboardShortcut("o", modifiers: .command)

                Button {
                    // Clone repo flow
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "arrow.down.circle")
                        Text("Clone Repository")
                    }
                    .frame(width: 200)
                }
                .buttonStyle(NeonButtonStyle(color: AtlasColors.neonPurple.opacity(0.6)))
            }

            Spacer()

            // Version
            Text("v0.1.0")
                .atlasFont(.caption)
                .foregroundStyle(AtlasColors.textTertiary)
                .padding(.bottom, 20)
        }
        .frame(width: 320)
        .padding(.horizontal, 40)
    }

    // MARK: - Right Panel (Recent Projects)

    private var rightPanel: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Recent Projects")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(AtlasColors.textSecondary)
                .padding(.top, 24)
                .padding(.horizontal, 24)

            if appState.recentProjects.isEmpty {
                Spacer()
                VStack(spacing: 12) {
                    Image(systemName: "folder")
                        .font(.system(size: 32, weight: .light))
                        .foregroundStyle(AtlasColors.textTertiary)
                    Text("No recent projects")
                        .atlasFont(.body)
                        .foregroundStyle(AtlasColors.textTertiary)
                }
                .frame(maxWidth: .infinity)
                Spacer()
            } else {
                ScrollView {
                    LazyVStack(spacing: 4) {
                        ForEach(appState.recentProjects) { project in
                            RecentProjectRow(project: project) {
                                appState.openProject(path: project.path)
                            }
                        }
                    }
                    .padding(.horizontal, 16)
                }
            }
        }
        .frame(maxWidth: .infinity)
        .background(AtlasColors.backgroundSurface.opacity(0.3))
    }
}

// MARK: - Recent Project Row

struct RecentProjectRow: View {
    let project: ProjectInfo
    let action: () -> Void
    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(AtlasColors.neonPurple.opacity(0.2))
                    .frame(width: 32, height: 32)
                    .overlay {
                        Image(systemName: "folder.fill")
                            .font(.system(size: 14))
                            .foregroundStyle(AtlasColors.neonPurple)
                    }

                VStack(alignment: .leading, spacing: 2) {
                    Text(project.name)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(AtlasColors.textPrimary)

                    Text(project.path)
                        .font(.system(size: 11))
                        .foregroundStyle(AtlasColors.textTertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }

                Spacer()

                Text(project.lastOpened, style: .relative)
                    .font(.system(size: 11))
                    .foregroundStyle(AtlasColors.textTertiary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(isHovered ? AtlasColors.sidebarHover : .clear)
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
