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
                    .foregroundStyle(.accent)
                Text(session.adapter)
                    .font(.headline)
                Spacer()
                statusBadge
                Button("Stop", systemImage: "stop.fill") {
                    Task { await appState.stopAgent(sessionId: session.id) }
                }
                .buttonStyle(.bordered)
                .tint(.red)
            }
            .padding()

            Divider()

            // Terminal
            if let terminalId = session.terminalSessionId {
                TerminalTabView(sessionId: terminalId)
            } else {
                ContentUnavailableView(
                    "No terminal",
                    systemImage: "terminal",
                    description: Text("This agent session has no terminal attached")
                )
            }

            Divider()

            // Prompt input
            HStack {
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
            .padding()
        }
    }

    private var statusBadge: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(statusColor)
                .frame(width: 8, height: 8)
            Text(session.activityState)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(.fill.tertiary, in: Capsule())
    }

    private var statusColor: Color {
        switch session.activityState {
        case "Active": .green
        case "Idle": .blue
        case "WaitingInput": .orange
        case "Blocked": .red
        default: .gray
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
