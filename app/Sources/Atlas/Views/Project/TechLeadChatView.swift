import SwiftUI

struct TechLeadChatView: View {
    @Environment(AppState.self) private var appState
    @State private var inputText = ""
    @State private var isSending = false
    @FocusState private var inputFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            chatHeader
            Divider().background(AtlasColors.border)
            messageList
            Divider().background(AtlasColors.border)
            inputBar
        }
        .background(AtlasColors.backgroundDeep)
    }

    // MARK: - Header

    private var chatHeader: some View {
        HStack(spacing: 10) {
            ZStack {
                Circle()
                    .fill(AtlasColors.neonPurple.opacity(0.2))
                    .frame(width: 32, height: 32)

                Image(systemName: "brain.head.profile")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(AtlasColors.neonPurple)
            }

            VStack(alignment: .leading, spacing: 2) {
                Text("Tech Lead")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(AtlasColors.textPrimary)

                HStack(spacing: 4) {
                    Circle()
                        .fill(appState.isTechLeadTyping ? AtlasColors.neonAmber : AtlasColors.neonGreen)
                        .frame(width: 6, height: 6)
                    Text(appState.isTechLeadTyping ? "Typing..." : "Online")
                        .font(.system(size: 11))
                        .foregroundStyle(AtlasColors.textTertiary)
                }
            }

            Spacer()

            Button {
                // Show terminal view
            } label: {
                Image(systemName: "terminal")
                    .font(.system(size: 13))
                    .foregroundStyle(AtlasColors.textSecondary)
                    .padding(6)
                    .background(AtlasColors.backgroundGlass)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            .buttonStyle(.plain)
            .help("View Terminal")
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
        .background(AtlasColors.backgroundSurface.opacity(0.5))
    }

    // MARK: - Messages

    private var messageList: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 16) {
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
                .padding(20)
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
        VStack(spacing: 16) {
            Spacer(minLength: 80)
            Image(systemName: "bubble.left.and.bubble.right")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(AtlasColors.textTertiary)
                .symbolRenderingMode(.hierarchical)

            VStack(spacing: 6) {
                Text("Tech Lead AI")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(AtlasColors.textPrimary)
                Text("Architecture decisions, code review, and technical guidance.")
                    .atlasFont(.body)
                    .foregroundStyle(AtlasColors.textSecondary)
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
                ForEach(0..<3) { i in
                    Circle()
                        .fill(AtlasColors.neonPurple.opacity(0.6))
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
            .background(AtlasColors.backgroundElevated)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            Spacer()
        }
    }

    // MARK: - Input Bar

    private var inputBar: some View {
        HStack(spacing: 10) {
            TextField("Ask the Tech Lead...", text: $inputText, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...5)
                .focused($inputFocused)
                .foregroundStyle(AtlasColors.textPrimary)
                .onSubmit { sendMessage() }

            Button {
                sendMessage()
            } label: {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.system(size: 24))
                    .foregroundStyle(
                        inputText.trimmingCharacters(in: .whitespaces).isEmpty
                            ? AtlasColors.textTertiary
                            : AtlasColors.neonCyan
                    )
                    .shadow(
                        color: inputText.trimmingCharacters(in: .whitespaces).isEmpty
                            ? .clear
                            : AtlasColors.neonCyan.opacity(0.3),
                        radius: 6
                    )
            }
            .buttonStyle(.plain)
            .disabled(inputText.trimmingCharacters(in: .whitespaces).isEmpty || isSending)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(AtlasColors.backgroundSurface.opacity(0.5))
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

// MARK: - Tech Lead Bubble

struct TechLeadBubble: View {
    let message: ChatMessage

    var body: some View {
        HStack {
            if message.role == .user { Spacer(minLength: 60) }

            VStack(alignment: message.role == .user ? .trailing : .leading, spacing: 4) {
                Text(message.content)
                    .font(.system(size: 13))
                    .foregroundStyle(AtlasColors.textPrimary)
                    .textSelection(.enabled)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(bubbleBackground, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .overlay {
                        if message.role == .assistant {
                            RoundedRectangle(cornerRadius: 14, style: .continuous)
                                .strokeBorder(AtlasColors.neonPurple.opacity(0.2), lineWidth: 0.5)
                        }
                    }

                Text(message.timestamp, style: .time)
                    .font(.system(size: 10))
                    .foregroundStyle(AtlasColors.textTertiary)
            }

            if message.role != .user { Spacer(minLength: 60) }
        }
    }

    private var bubbleBackground: Color {
        switch message.role {
        case .user: AtlasColors.neonCyan.opacity(0.12)
        case .assistant: AtlasColors.backgroundElevated
        case .system: AtlasColors.neonRed.opacity(0.1)
        }
    }
}
