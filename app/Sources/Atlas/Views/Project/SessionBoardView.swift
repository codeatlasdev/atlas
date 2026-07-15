import SwiftUI

// MARK: - Session Board (replaces Kanban)

struct SessionBoardView: View {
    @Environment(AppState.self) private var appState
    @State private var showSpawnWorker = false
    @State private var selectedSession: AgentSessionInfo?

    var body: some View {
        VStack(spacing: 0) {
            boardHeader
            SoftDivider()
            boardContent
        }
        .background(DS.bg.base)
        .sheet(isPresented: $showSpawnWorker) {
            SpawnWorkerSheet()
        }
        .sheet(item: $selectedSession) { session in
            SessionInspectorSheet(session: session)
                .frame(minWidth: 700, minHeight: 500)
        }
        .task {
            // Periodic refresh every 5 seconds
            while !Task.isCancelled {
                await appState.refreshAgentSessions()
                try? await Task.sleep(for: .seconds(5))
            }
        }
    }

    // MARK: - Header

    private var boardHeader: some View {
        HStack(spacing: DS.spacing.md) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Sessions")
                    .font(.atlasTitle)
                    .foregroundStyle(DS.text.primary)

                Text("Agents flowing from work → review → merge")
                    .font(.system(size: 11))
                    .foregroundStyle(DS.text.tertiary)
            }

            Spacer()

            // Active count pill
            if !workingSessions.isEmpty {
                HStack(spacing: 4) {
                    Circle()
                        .fill(DS.status.success)
                        .frame(width: 6, height: 6)
                    Text("\(workingSessions.count) working")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(DS.text.secondary)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(DS.status.success.opacity(0.1))
                .clipShape(Capsule())
            }

            AtlasButton("Spawn Worker", icon: "plus") {
                showSpawnWorker = true
            }
        }
        .padding(.horizontal, DS.spacing.xxl)
        .padding(.vertical, 14)
    }

    // MARK: - Board

    private var boardContent: some View {
        Group {
            if appState.agentSessions.isEmpty {
                emptyBoard
            } else {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(alignment: .top, spacing: DS.spacing.lg) {
                        BoardColumn(
                            title: "Working",
                            color: DS.status.success,
                            sessions: workingSessions,
                            onSelect: { selectedSession = $0 }
                        )
                        BoardColumn(
                            title: "Needs You",
                            color: DS.status.warning,
                            sessions: needsYouSessions,
                            onSelect: { selectedSession = $0 }
                        )
                        BoardColumn(
                            title: "Done",
                            color: DS.text.tertiary,
                            sessions: doneSessions,
                            onSelect: { selectedSession = $0 }
                        )
                    }
                    .padding(DS.spacing.xl)
                }
            }
        }
    }

    private var emptyBoard: some View {
        VStack(spacing: DS.spacing.xl) {
            Spacer()
            Image(systemName: "square.grid.3x3.topleft.filled")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(DS.text.tertiary)
                .symbolRenderingMode(.hierarchical)

            VStack(spacing: 6) {
                Text("No Active Sessions")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(DS.text.primary)
                Text("Spawn a worker agent to start coding in parallel.")
                    .font(.system(size: 13))
                    .foregroundStyle(DS.text.secondary)
            }

            AtlasButton("Spawn Worker", icon: "plus") {
                showSpawnWorker = true
            }
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    // MARK: - Session Grouping (derived from state)

    private var workingSessions: [AgentSessionInfo] {
        appState.agentSessions.filter { $0.activityState == "Active" }
    }

    private var needsYouSessions: [AgentSessionInfo] {
        appState.agentSessions.filter {
            $0.activityState == "WaitingInput" || $0.activityState == "Blocked"
        }
    }

    private var reviewSessions: [AgentSessionInfo] {
        // TODO: check PR state when we add SCM integration
        []
    }

    private var doneSessions: [AgentSessionInfo] {
        appState.agentSessions.filter {
            $0.activityState == "Idle" || $0.activityState.contains("Exited")
        }
    }
}

// MARK: - Board Column

struct BoardColumn: View {
    let title: String
    let color: Color
    let sessions: [AgentSessionInfo]
    let onSelect: (AgentSessionInfo) -> Void

    var body: some View {
        VStack(spacing: DS.spacing.md) {
            // Column header
            HStack(spacing: DS.spacing.sm) {
                Circle()
                    .fill(color)
                    .frame(width: 8, height: 8)

                Text(title)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(DS.text.primary)

                Spacer()

                if !sessions.isEmpty {
                    Text("\(sessions.count)")
                        .font(.system(size: 11, weight: .medium, design: .rounded))
                        .foregroundStyle(DS.text.tertiary)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(DS.bg.hover)
                        .clipShape(Capsule())
                }
            }
            .padding(.horizontal, 14)
            .padding(.top, 14)
            .padding(.bottom, DS.spacing.xs)

            // Cards
            ScrollView {
                LazyVStack(spacing: DS.spacing.sm) {
                    ForEach(sessions) { session in
                        SessionCard(session: session)
                            .onTapGesture { onSelect(session) }
                    }
                }
                .padding(.horizontal, 10)
                .padding(.bottom, 14)
            }
        }
        .frame(width: 260)
        .background(
            RoundedRectangle(cornerRadius: DS.radius.lg, style: .continuous)
                .fill(DS.bg.elevated.opacity(0.4))
        )
    }
}

// MARK: - Session Card

struct SessionCard: View {
    let session: AgentSessionInfo
    @State private var isHovered = false
    @State private var pulseOpacity: Double = 1.0

    var body: some View {
        VStack(alignment: .leading, spacing: DS.spacing.sm) {
            // Top row: status + title/agent name
            HStack(spacing: DS.spacing.sm) {
                // Status indicator
                if session.activityState == "Active" {
                    Circle()
                        .fill(DS.status.success)
                        .frame(width: 7, height: 7)
                        .opacity(pulseOpacity)
                        .animation(
                            .easeInOut(duration: 1.2).repeatForever(autoreverses: true),
                            value: pulseOpacity
                        )
                        .onAppear { pulseOpacity = 0.4 }
                } else {
                    Circle()
                        .fill(statusColor)
                        .frame(width: 7, height: 7)
                }

                Text(session.title ?? session.adapter)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(DS.text.primary)
                    .lineLimit(1)

                Spacer()

                // Adapter badge (small)
                if session.title != nil {
                    Text(session.adapter)
                        .font(.system(size: 9, weight: .medium, design: .monospaced))
                        .foregroundStyle(DS.text.tertiary)
                        .padding(.horizontal, 4)
                        .padding(.vertical, 1)
                        .background(DS.bg.hover)
                        .clipShape(RoundedRectangle(cornerRadius: 3))
                }
            }

            // Session ID (mono)
            Text("session/\(session.id.prefix(8))")
                .font(.system(size: 10, design: .monospaced))
                .foregroundStyle(DS.text.tertiary)

            // Activity state text
            Text(activityLabel)
                .font(.system(size: 11))
                .foregroundStyle(statusColor)
        }
        .padding(DS.spacing.md)
        .background(DS.bg.elevated)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(
                    isHovered ? DS.accent.primary.opacity(0.3) : DS.border.subtle,
                    lineWidth: 0.5
                )
        )
        .contentShape(Rectangle())
        .onHover { hovering in
            withAnimation(.easeInOut(duration: 0.15)) {
                isHovered = hovering
            }
        }
    }

    private var statusColor: Color {
        switch session.activityState {
        case "Active": DS.status.success
        case "WaitingInput": DS.status.warning
        case "Blocked": DS.status.error
        case "Idle": DS.text.tertiary
        default: DS.text.tertiary
        }
    }

    private var activityLabel: String {
        switch session.activityState {
        case "Active": return "Working..."
        case "WaitingInput": return "Needs input"
        case "Blocked": return "Blocked"
        case "Idle": return "Idle"
        default:
            if session.activityState.contains("Exited") { return "Exited" }
            return session.activityState
        }
    }
}

// MARK: - Spawn Worker Sheet

struct SpawnWorkerSheet: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss
    @State private var taskPrompt = ""
    @State private var selectedAdapter = "kiro"
    @State private var isSpawning = false

    private let adapters = ["kiro", "claude-code", "codex", "aider"]

    var body: some View {
        VStack(spacing: DS.spacing.xl) {
            // Header
            HStack {
                Text("SPAWN WORKER")
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
                    .foregroundStyle(DS.text.tertiary)
                    .tracking(1.2)
                Spacer()
                Button { dismiss() } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(DS.text.tertiary)
                }
                .buttonStyle(.plain)
            }

            // Task prompt
            VStack(alignment: .leading, spacing: DS.spacing.sm) {
                Text("Task")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(DS.text.secondary)

                TextField("What should this agent work on?", text: $taskPrompt, axis: .vertical)
                    .textFieldStyle(.plain)
                    .font(.system(size: 14))
                    .lineLimit(3...6)
                    .padding(12)
                    .background(DS.bg.base)
                    .clipShape(RoundedRectangle(cornerRadius: DS.radius.md))
                    .overlay(
                        RoundedRectangle(cornerRadius: DS.radius.md)
                            .strokeBorder(DS.border.medium, lineWidth: 0.5)
                    )
            }

            // Agent selector
            VStack(alignment: .leading, spacing: DS.spacing.sm) {
                Text("Agent")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(DS.text.secondary)

                HStack(spacing: DS.spacing.sm) {
                    ForEach(adapters, id: \.self) { adapter in
                        Button {
                            selectedAdapter = adapter
                        } label: {
                            Text(adapter)
                                .font(.system(size: 11, weight: .medium))
                                .foregroundStyle(
                                    selectedAdapter == adapter ? DS.text.primary : DS.text.tertiary
                                )
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                                .background(
                                    selectedAdapter == adapter
                                        ? DS.accent.subtle
                                        : DS.bg.hover
                                )
                                .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm))
                                .overlay(
                                    RoundedRectangle(cornerRadius: DS.radius.sm)
                                        .strokeBorder(
                                            selectedAdapter == adapter
                                                ? DS.accent.primary.opacity(0.3)
                                                : .clear,
                                            lineWidth: 1
                                        )
                                )
                        }
                        .buttonStyle(.plain)
                    }
                }
            }

            Spacer()

            // Spawn button
            HStack {
                Spacer()
                Button {
                    spawnWorker()
                } label: {
                    HStack(spacing: 6) {
                        if isSpawning {
                            ProgressView()
                                .controlSize(.small)
                        }
                        Text("Spawn Worker")
                            .font(.system(size: 13, weight: .medium))
                        Text("⌘↵")
                            .font(.system(size: 10, design: .monospaced))
                            .foregroundStyle(.white.opacity(0.5))
                    }
                    .foregroundStyle(.white)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .background(DS.accent.primary)
                    .clipShape(RoundedRectangle(cornerRadius: DS.radius.md))
                }
                .buttonStyle(.plain)
                .disabled(taskPrompt.isEmpty || isSpawning)
                .keyboardShortcut(.return, modifiers: .command)
            }
        }
        .padding(DS.spacing.xxl)
        .frame(width: 440, height: 320)
        .background(DS.bg.elevated)
    }

    private func spawnWorker() {
        guard let project = appState.currentProject else { return }
        isSpawning = true
        Task {
            do {
                let _ = try await appState.daemon.send(method: "agent.spawn", params: [
                    "adapter": selectedAdapter,
                    "prompt": taskPrompt,
                    "cwd": project.path,
                    "permission": "autonomous"
                ])
                await appState.refreshAgentSessions()
            } catch {
                // TODO: show error
            }
            dismiss()
        }
    }
}

// MARK: - Session Inspector Sheet

struct SessionInspectorSheet: View {
    @Environment(AppState.self) private var appState
    let session: AgentSessionInfo
    @State private var activitySession: AgentActivitySession?

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack(spacing: DS.spacing.md) {
                Circle()
                    .fill(session.activityState == "Active" ? DS.status.success : DS.text.tertiary)
                    .frame(width: 8, height: 8)

                Text(session.title ?? session.adapter)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(DS.text.primary)

                if session.title != nil {
                    Text(session.adapter)
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundStyle(DS.text.tertiary)
                }

                Spacer()

                Text(session.activityState)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(DS.text.secondary)
            }
            .padding(DS.spacing.lg)

            SoftDivider()

            // Activity Feed — subscribe and show real events
            if let activity = activitySession {
                AgentActivityView(
                    session: activity,
                    onSendMessage: { msg in
                        Task {
                            let _ = try? await appState.daemon.send(
                                method: "agent.prompt",
                                params: ["session_id": session.id, "prompt": msg]
                            )
                        }
                    },
                    onPermissionRespond: { reqId, optionId in
                        Task {
                            let _ = try? await appState.daemon.send(
                                method: "agent.permission",
                                params: [
                                    "session_id": session.id,
                                    "request_id": "\(reqId)",
                                    "option_id": optionId
                                ]
                            )
                        }
                    }
                )
            } else {
                VStack {
                    Spacer()
                    ProgressView()
                        .controlSize(.small)
                    Text("Connecting to session...")
                        .font(.atlasCaption)
                        .foregroundStyle(DS.text.tertiary)
                    Spacer()
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(DS.bg.base)
            }
        }
        .background(DS.bg.elevated)
        .task {
            // Create activity session and subscribe
            let activity = await AgentActivitySession(
                sessionId: session.id,
                adapterName: session.title ?? session.adapter
            )
            activitySession = activity

            // Subscribe to events
            let _ = try? await appState.daemon.send(
                method: "agent.subscribe",
                params: ["session_id": session.id]
            )

            // Listen for events
            appState.daemon.onNotification("agent.event", id: "inspector-\(session.id)") { payload in
                guard let eventData = try? JSONSerialization.data(withJSONObject: payload.params),
                      let event = try? JSONDecoder().decode(AgentEvent.self, from: eventData),
                      event.sessionId == session.id else { return }

                DispatchQueue.main.async {
                    activity.apply(event: event)
                }
            }
        }
    }
}
