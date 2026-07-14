import SwiftUI

struct MessageBubble: View {
    let message: ChatMessage

    var body: some View {
        HStack {
            if message.role == .user { Spacer(minLength: 60) }

            VStack(alignment: message.role == .user ? .trailing : .leading, spacing: 4) {
                Text(message.content)
                    .atlasFont(.body)
                    .atlasForeground(.primary)
                    .textSelection(.enabled)
                    .padding(12)
                    .background(bubbleBackground, in: RoundedRectangle(cornerRadius: 12, style: .continuous))

                Text(message.timestamp, style: .time)
                    .atlasFont(.caption)
                    .atlasForeground(.tertiary)
            }

            if message.role != .user { Spacer(minLength: 60) }
        }
    }

    private var bubbleBackground: Color {
        switch message.role {
        case .user: AtlasColors.accentPrimary.opacity(0.15)
        case .assistant: AtlasColors.backgroundSurface
        case .system: AtlasColors.statusError.opacity(0.1)
        }
    }
}
