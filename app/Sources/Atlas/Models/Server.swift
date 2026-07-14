import Foundation
import SwiftUI

struct Server: Codable, Identifiable, Hashable {
    let id: UUID
    var name: String
    var host: String
    var user: String
    var port: UInt16
    var status: ServerStatus
    let createdAt: Date
    var updatedAt: Date

    enum CodingKeys: String, CodingKey {
        case id, name, host, user, port, status
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

enum ServerStatus: String, Codable, Hashable {
    case online
    case offline
    case unreachable
    case unknown

    var label: String {
        rawValue.capitalized
    }

    var systemImage: String {
        switch self {
        case .online: "circle.fill"
        case .offline: "circle"
        case .unreachable: "exclamationmark.circle"
        case .unknown: "questionmark.circle"
        }
    }

    var tintColor: Color {
        switch self {
        case .online: AtlasColors.statusSuccess
        case .offline: AtlasColors.textTertiary
        case .unreachable: AtlasColors.statusError
        case .unknown: AtlasColors.textSecondary
        }
    }
}
