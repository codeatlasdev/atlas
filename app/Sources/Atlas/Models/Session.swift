import Foundation

struct Session: Codable, Identifiable, Hashable {
    let id: UUID
    var kind: SessionKind
    var serverId: UUID?
    let startedAt: Date
    var endedAt: Date?
    var metadata: [String: String]

    enum CodingKeys: String, CodingKey {
        case id, kind, metadata
        case serverId = "server_id"
        case startedAt = "started_at"
        case endedAt = "ended_at"
    }

    var isActive: Bool {
        endedAt == nil
    }

    var durationLabel: String {
        let start = startedAt
        let end = endedAt ?? .now
        let duration = end.timeIntervalSince(start)

        if duration < 60 { return "\(Int(duration))s" }
        if duration < 3600 { return "\(Int(duration / 60))m" }
        return "\(Int(duration / 3600))h \(Int((duration.truncatingRemainder(dividingBy: 3600)) / 60))m"
    }
}

enum SessionKind: String, Codable, Hashable {
    case ssh
    case ai

    var label: String {
        switch self {
        case .ssh: "SSH"
        case .ai: "AI"
        }
    }

    var systemImage: String {
        switch self {
        case .ssh: "terminal"
        case .ai: "bubble.left.and.bubble.right"
        }
    }
}
