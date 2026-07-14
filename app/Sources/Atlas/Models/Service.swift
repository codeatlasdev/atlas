import Foundation
import SwiftUI

struct SystemdService: Codable, Identifiable, Hashable {
    let id: UUID
    let serverId: UUID
    var name: String
    var unitName: String
    var state: ServiceState
    var enabled: Bool
    let createdAt: Date

    enum CodingKeys: String, CodingKey {
        case id, name, state, enabled
        case serverId = "server_id"
        case unitName = "unit_name"
        case createdAt = "created_at"
    }
}

enum ServiceState: String, Codable, Hashable {
    case running
    case stopped
    case failed
    case restarting
    case unknown

    var label: String {
        rawValue.capitalized
    }

    var systemImage: String {
        switch self {
        case .running: "play.circle.fill"
        case .stopped: "stop.circle"
        case .failed: "xmark.circle.fill"
        case .restarting: "arrow.clockwise.circle"
        case .unknown: "questionmark.circle"
        }
    }

    var tintColor: Color {
        switch self {
        case .running: AtlasColors.statusSuccess
        case .stopped: AtlasColors.textTertiary
        case .failed: AtlasColors.statusError
        case .restarting: AtlasColors.accentPrimary
        case .unknown: AtlasColors.textSecondary
        }
    }
}
