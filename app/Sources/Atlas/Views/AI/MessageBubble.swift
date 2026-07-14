import SwiftUI

struct MessageBubble: View {
    let message: ChatMessage

    var body: some View {
        HStack {
            if message.role == .user { Spacer(minLength: 60) }

            VStack(alignment: message.role == .user ? .trailing : .leading, spacing: 4) {
                Text(message.content)
                    .atlasFont(.body)
                    .textSelection(.enabled)
                    .padding(10)
                    .background(bubbleBackground, in: RoundedRectangle(cornerRadius: 12))

                Text(message.timestamp, style: .time)
                    .atlasFont(.caption)
                    .foregroundStyle(.textSecondary)
            }

            if message.role != .user { Spacer(minLength: 60) }
        }
    }

    private var bubbleBackground: Color {
        switch message.role {
        case .user: .atlas(.accent).opacity(0.15)
        case .assistant: .atlas(.surface)
        case .system: .atlas(.statusError).opacity(0.1)
        }
    }
}
