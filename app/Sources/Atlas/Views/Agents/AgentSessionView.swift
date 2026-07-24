import SwiftUI

struct AgentSessionView: View {
    let session: AgentSessionInfo
    @Environment(AppState.self) private var appState
    @State private var promptText = ""

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Image(systemName: "cpu")
                    .foregroundStyle(AtlasColors.accentPrimary)
                Text(session.adapter)
                    .atlasFont(.headline)
                    .atlasForeground(.primary)
                Spacer()
                statusBadge
                Button("Stop", systemImage: "stop.fill") {
                    Task { await appState.stopAgent(sessionId: session.id) }
                }
                .buttonStyle(.bordered)
                .tint(AtlasColors.statusError)
            }
            .padding(16)

            Divider().opacity(0.4)

            // Terminal
            if let terminalId = session.terminalSessionId {
                TerminalTabView(sessionId: terminalId)
            } else {
                EmptyStateView(
                    icon: "terminal",
                    title: "No Terminal",
                    description: "This agent session has no terminal attached."
                )
            }

            Divider().opacity(0.4)

            // Prompt input
            HStack(spacing: 10) {
                TextField("Send a follow-up prompt...", text: $promptText)
                    .textFieldStyle(.plain)
                    .onSubmit { sendPrompt() }

                Button("Send", systemImage: "paperplane.fill") {
                    sendPrompt()
                }
                .buttonStyle(.bordered)
                .disabled(promptText.isEmpty)
                .keyboardShortcut(.return, modifiers: .command)
            }
            .padding(14)
            .background(.ultraThinMaterial)
        }
    }

    private var statusBadge: some View {
        StatusBadge(label: session.activityState, color: statusColor)
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

    private func sendPrompt() {
        guard !promptText.isEmpty else { return }
        let text = promptText
        promptText = ""
        Task {
            await appState.sendPromptToAgent(sessionId: session.id, prompt: text)
        }
    }
}
