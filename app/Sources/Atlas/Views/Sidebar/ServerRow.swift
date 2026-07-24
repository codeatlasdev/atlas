import SwiftUI

struct ServerRow: View {
    let server: Server

    var body: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(server.status.tintColor)
                .frame(width: 8, height: 8)

            VStack(alignment: .leading, spacing: 2) {
                Text(server.name)
                    .font(.system(size: 13, weight: .medium))
                    .atlasForeground(.primary)
                Text("\(server.user)@\(server.host)")
                    .atlasFont(.caption)
                    .atlasForeground(.tertiary)
            }

            Spacer()
        }
    }
}
