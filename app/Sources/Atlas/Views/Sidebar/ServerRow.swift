import SwiftUI

struct ServerRow: View {
    let server: Server

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: server.status.systemImage)
                .foregroundStyle(server.status.tint)
                .imageScale(.small)

            VStack(alignment: .leading, spacing: 2) {
                Text(server.name)
                    .atlasFont(.body)
                    .lineLimit(1)
                Text("\(server.user)@\(server.host)")
                    .atlasFont(.caption)
                    .foregroundStyle(.textSecondary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 2)
    }
}
