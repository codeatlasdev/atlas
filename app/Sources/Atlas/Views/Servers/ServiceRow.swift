import SwiftUI

struct ServiceRow: View {
    let service: SystemdService

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: service.state.systemImage)
                .foregroundStyle(service.state.tint)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                Text(service.name)
                    .atlasFont(.body)
                Text(service.unitName)
                    .atlasFont(.caption)
                    .foregroundStyle(.textSecondary)
            }

            Spacer()

            Text(service.enabled ? "enabled" : "disabled")
                .atlasFont(.caption)
                .foregroundStyle(.textSecondary)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(.atlas(.surface), in: RoundedRectangle(cornerRadius: 4))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(Color.clear)
        .contentShape(Rectangle())
    }
}
