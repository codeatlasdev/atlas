import SwiftUI

struct MessageBubble: View {
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
                    .background(bubbleBackground, in: RoundedRectangle(cornerRadius: 12, style: .continuous))

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
