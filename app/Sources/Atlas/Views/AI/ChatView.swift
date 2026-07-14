import SwiftUI

struct ChatView: View {
    @Environment(AppState.self) private var appState
    @State private var inputText = ""
    @State private var isSending = false
    @FocusState private var inputFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            messageList
            Divider().opacity(0.4)
            inputBar
        }
    }

    private var messageList: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 12) {
                    if appState.messages.isEmpty {
                        EmptyStateView(
                            icon: "bubble.left.and.bubble.right",
                            title: "Tech Lead",
                            description: "Chat with your AI agent for architecture decisions, code review, and technical guidance."
                        )
                        .frame(maxHeight: .infinity)
                    } else {
                        ForEach(appState.messages) { message in
                            MessageBubble(message: message)
                                .id(message.id)
                        }
                    }
                }
                .padding(20)
            }
            .onChange(of: appState.messages.count) {
                if let last = appState.messages.last {
                    withAnimation(.easeOut(duration: 0.2)) {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
        }
    }

    private var inputBar: some View {
        HStack(spacing: 10) {
            TextField("Ask anything...", text: $inputText, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...5)
                .focused($inputFocused)
                .onSubmit { sendMessage() }

            Button {
                sendMessage()
            } label: {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.system(size: 22))
            }
            .disabled(inputText.trimmingCharacters(in: .whitespaces).isEmpty || isSending)
            .buttonStyle(.plain)
            .foregroundStyle(
                inputText.trimmingCharacters(in: .whitespaces).isEmpty
                    ? AtlasColors.textTertiary
                    : AtlasColors.accentPrimary
            )
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(.ultraThinMaterial)
    }

    private func sendMessage() {
        let text = inputText.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty, !isSending else { return }

        inputText = ""
        isSending = true

        Task {
            await appState.sendChat(message: text)
            isSending = false
            inputFocused = true
        }
    }
}
