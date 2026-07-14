import SwiftUI

struct TechLeadChatView: View {
    @Environment(AppState.self) private var appState
    @State private var inputText = ""
    @State private var isSending = false
    @FocusState private var inputFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            chatHeader
            SoftDivider()
            messageList
            SoftDivider()
            inputBar
        }
        .background(DS.bg.base)
    }

    // MARK: - Header

    private var chatHeader: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(DS.accent.subtle)
                .frame(width: 32, height: 32)
                .overlay {
                    Image(systemName: "brain.head.profile")
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(DS.accent.primary)
                }

            VStack(alignment: .leading, spacing: 2) {
                Text("Tech Lead")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(DS.text.primary)

                HStack(spacing: DS.spacing.xs) {
                    Circle()
                        .fill(appState.isTechLeadTyping ? DS.status.warning : DS.status.success)
                        .frame(width: 6, height: 6)
                    Text(appState.isTechLeadTyping ? "Typing..." : "Online")
                        .font(.system(size: 11))
                        .foregroundStyle(DS.text.tertiary)
                }
            }

            Spacer()

            Button {
                // Show terminal view
            } label: {
                Image(systemName: "terminal")
                    .font(.system(size: 13))
                    .foregroundStyle(DS.text.secondary)
                    .padding(6)
                    .background(DS.bg.hover)
                    .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm))
            }
            .buttonStyle(.plain)
            .help("View Terminal")
        }
        .padding(.horizontal, DS.spacing.xl)
        .padding(.vertical, DS.spacing.md)
        .background(DS.bg.elevated.opacity(0.5))
    }

    // MARK: - Messages

    private var messageList: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: DS.spacing.lg) {
                    if appState.techLeadMessages.isEmpty {
                        emptyState
                    } else {
                        ForEach(appState.techLeadMessages) { message in
                            TechLeadBubble(message: message)
                                .id(message.id)
                        }

                        if appState.isTechLeadTyping {
                            typingIndicator
                        }
                    }
                }
                .padding(DS.spacing.xl)
            }
            .onChange(of: appState.techLeadMessages.count) {
                if let last = appState.techLeadMessages.last {
                    withAnimation(.easeOut(duration: 0.2)) {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
        }
    }

    private var emptyState: some View {
        VStack(spacing: DS.spacing.lg) {
            Spacer(minLength: 80)
            Image(systemName: "bubble.left.and.bubble.right")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(DS.text.tertiary)
                .symbolRenderingMode(.hierarchical)

            VStack(spacing: 6) {
                Text("Tech Lead AI")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(DS.text.primary)
                Text("Architecture decisions, code review, and technical guidance.")
                    .font(.atlasBody)
                    .foregroundStyle(DS.text.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 300)
            }
            Spacer(minLength: 80)
        }
        .frame(maxWidth: .infinity)
    }

    private var typingIndicator: some View {
        HStack {
            HStack(spacing: 4) {
                ForEach(0..<3, id: \.self) { i in
                    Circle()
                        .fill(DS.accent.primary.opacity(0.6))
                        .frame(width: 6, height: 6)
                        .animation(
                            .easeInOut(duration: 0.6)
                                .repeatForever()
                                .delay(Double(i) * 0.2),
                            value: appState.isTechLeadTyping
                        )
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(DS.bg.elevated)
            .clipShape(RoundedRectangle(cornerRadius: DS.radius.lg, style: .continuous))
            Spacer()
        }
    }

    // MARK: - Input Bar

    private var inputBar: some View {
        HStack(spacing: 10) {
            TextField("Ask the Tech Lead...", text: $inputText, axis: .vertical)
                .textFieldStyle(.plain)
                .font(.atlasBody)
                .lineLimit(1...5)
                .focused($inputFocused)
                .foregroundStyle(DS.text.primary)
                .onSubmit { sendMessage() }

            Button {
                sendMessage()
            } label: {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.system(size: 24))
                    .foregroundStyle(
                        inputText.trimmingCharacters(in: .whitespaces).isEmpty
                            ? DS.text.tertiary
                            : DS.accent.primary
                    )
            }
            .buttonStyle(.plain)
            .disabled(inputText.trimmingCharacters(in: .whitespaces).isEmpty || isSending)
        }
        .padding(.horizontal, DS.spacing.lg)
        .padding(.vertical, DS.spacing.md)
        .background(DS.bg.elevated.opacity(0.5))
    }

    private func sendMessage() {
        let text = inputText.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty, !isSending else { return }

        inputText = ""
        isSending = true

        Task {
            await appState.sendToTechLead(message: text)
            isSending = false
            inputFocused = true
        }
    }
}

// MARK: - Bubble

struct TechLeadBubble: View {
    let message: ChatMessage

    var body: some View {
        HStack {
            if message.role == .user { Spacer(minLength: 60) }

            VStack(alignment: message.role == .user ? .trailing : .leading, spacing: DS.spacing.xs) {
                Text(message.content)
                    .font(.system(size: 13))
                    .foregroundStyle(DS.text.primary)
                    .textSelection(.enabled)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(bubbleBackground)
                    .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .overlay(
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .strokeBorder(
                                message.role == .assistant ? DS.border.subtle : .clear,
                                lineWidth: 0.5
                            )
                    )

                Text(message.timestamp, style: .time)
                    .font(.system(size: 10))
                    .foregroundStyle(DS.text.tertiary)
            }

            if message.role != .user { Spacer(minLength: 60) }
        }
    }

    private var bubbleBackground: Color {
        switch message.role {
        case .user: DS.accent.subtle
        case .assistant: DS.bg.elevated
        case .system: DS.status.error.opacity(0.1)
        }
    }
}
