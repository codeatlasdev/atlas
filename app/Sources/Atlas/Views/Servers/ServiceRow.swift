import SwiftUI

struct ServiceRow: View {
    let service: SystemdService

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: service.state.systemImage)
                .foregroundStyle(service.state.tintColor)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                Text(service.name)
                    .atlasFont(.body)
                    .atlasForeground(.primary)
                Text(service.unitName)
                    .atlasFont(.caption)
                    .atlasForeground(.secondary)
            }

            Spacer()

            StatusBadge(
                label: service.enabled ? "enabled" : "disabled",
                color: service.enabled ? AtlasColors.statusSuccess : AtlasColors.textTertiary,
                showDot: false
            )
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .contentShape(Rectangle())
    }
}
