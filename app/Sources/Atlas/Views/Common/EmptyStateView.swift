import SwiftUI

struct EmptyStateView: View {
    let icon: String
    let title: String
    let description: String
    var actionLabel: String?
    var action: (() -> Void)?

    var body: some View {
        VStack(spacing: 20) {
            Spacer()

            Image(systemName: icon)
                .font(.system(size: 48, weight: .light))
                .foregroundStyle(AtlasColors.textTertiary)
                .symbolRenderingMode(.hierarchical)

            VStack(spacing: 8) {
                Text(title)
                    .font(.system(size: 20, weight: .semibold))
                    .foregroundStyle(AtlasColors.textPrimary)

                Text(description)
                    .atlasFont(.body)
                    .foregroundStyle(AtlasColors.textSecondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 280)
            }

            if let actionLabel, let action {
                Button(action: action) {
                    HStack(spacing: 6) {
                        Image(systemName: "plus")
                            .font(.system(size: 11, weight: .semibold))
                        Text(actionLabel)
                            .font(.system(size: 13, weight: .medium))
                    }
                }
                .buttonStyle(NeonButtonStyle(color: AtlasColors.neonCyan))
            }

            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(AtlasColors.backgroundDeep)
    }
}
