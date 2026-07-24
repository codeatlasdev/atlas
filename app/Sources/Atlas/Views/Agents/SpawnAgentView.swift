import SwiftUI

struct SpawnAgentView: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss
    @State private var prompt = ""
    @State private var selectedAdapter = "kiro"
    @State private var cwd = "~"
    @State private var isSpawning = false

    let adapters = ["kiro", "claude-code", "aider", "codex"]

    var body: some View {
        VStack(spacing: 16) {
            Text("Spawn Agent")
                .font(.title2)
                .fontWeight(.semibold)

            Form {
                Picker("Agent", selection: $selectedAdapter) {
                    ForEach(adapters, id: \.self) { adapter in
                        Text(adapter).tag(adapter)
                    }
                }

                TextField("Working Directory", text: $cwd)

                TextField("Prompt", text: $prompt, axis: .vertical)
                    .lineLimit(3...8)
            }
            .formStyle(.grouped)

            HStack {
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)

                Spacer()

                Button("Spawn") { spawn() }
                    .buttonStyle(.borderedProminent)
                    .disabled(prompt.isEmpty || isSpawning)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 480)
    }

    private func spawn() {
        isSpawning = true
        Task {
            _ = await appState.spawnAgent(
                adapter: selectedAdapter,
                prompt: prompt,
                cwd: cwd
            )
            isSpawning = false
            dismiss()
        }
    }
}
