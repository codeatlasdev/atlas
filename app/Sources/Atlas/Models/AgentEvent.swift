import Foundation

// MARK: - Agent Event (from daemon via JSON-RPC notifications)

struct AgentEvent: Codable, Identifiable {
    let sessionId: String
    let event: AgentEventKind
    let timestampMs: UInt64

    var id: String { "\(sessionId)-\(timestampMs)-\(event.kindName)" }

    enum CodingKeys: String, CodingKey {
        case sessionId = "session_id"
        case event
        case timestampMs = "timestamp_ms"
    }
}

enum AgentEventKind: Codable {
    case textChunk(TextChunk)
    case thinkingChunk(ThinkingChunk)
    case toolCallStart(ToolCallStart)
    case toolCallUpdate(ToolCallUpdate)
    case plan(Plan)
    case permissionRequest(PermissionRequest)
    case usageUpdate(UsageUpdate)
    case subagentSpawned(SubagentSpawned)
    case subagentCompleted(SubagentCompleted)
    case turnEnd(TurnEnd)
    case sessionStatus(SessionStatus)

    var kindName: String {
        switch self {
        case .textChunk: "text"
        case .thinkingChunk: "thinking"
        case .toolCallStart: "tool_start"
        case .toolCallUpdate: "tool_update"
        case .plan: "plan"
        case .permissionRequest: "permission"
        case .usageUpdate: "usage"
        case .subagentSpawned: "subagent_spawn"
        case .subagentCompleted: "subagent_done"
        case .turnEnd: "turn_end"
        case .sessionStatus: "status"
        }
    }

    enum CodingKeys: String, CodingKey {
        case kind, data
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(String.self, forKey: .kind)

        switch kind {
        case "TextChunk":
            self = .textChunk(try container.decode(TextChunk.self, forKey: .data))
        case "ThinkingChunk":
            self = .thinkingChunk(try container.decode(ThinkingChunk.self, forKey: .data))
        case "ToolCallStart":
            self = .toolCallStart(try container.decode(ToolCallStart.self, forKey: .data))
        case "ToolCallUpdate":
            self = .toolCallUpdate(try container.decode(ToolCallUpdate.self, forKey: .data))
        case "Plan":
            self = .plan(try container.decode(Plan.self, forKey: .data))
        case "PermissionRequest":
            self = .permissionRequest(try container.decode(PermissionRequest.self, forKey: .data))
        case "UsageUpdate":
            self = .usageUpdate(try container.decode(UsageUpdate.self, forKey: .data))
        case "SubagentSpawned":
            self = .subagentSpawned(try container.decode(SubagentSpawned.self, forKey: .data))
        case "SubagentCompleted":
            self = .subagentCompleted(try container.decode(SubagentCompleted.self, forKey: .data))
        case "TurnEnd":
            self = .turnEnd(try container.decode(TurnEnd.self, forKey: .data))
        case "SessionStatus":
            self = .sessionStatus(try container.decode(SessionStatus.self, forKey: .data))
        default:
            self = .textChunk(TextChunk(messageId: "unknown", text: "", isContinuation: false))
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .textChunk(let v):
            try container.encode("TextChunk", forKey: .kind)
            try container.encode(v, forKey: .data)
        case .thinkingChunk(let v):
            try container.encode("ThinkingChunk", forKey: .kind)
            try container.encode(v, forKey: .data)
        case .toolCallStart(let v):
            try container.encode("ToolCallStart", forKey: .kind)
            try container.encode(v, forKey: .data)
        case .toolCallUpdate(let v):
            try container.encode("ToolCallUpdate", forKey: .kind)
            try container.encode(v, forKey: .data)
        case .plan(let v):
            try container.encode("Plan", forKey: .kind)
            try container.encode(v, forKey: .data)
        case .permissionRequest(let v):
            try container.encode("PermissionRequest", forKey: .kind)
            try container.encode(v, forKey: .data)
        case .usageUpdate(let v):
            try container.encode("UsageUpdate", forKey: .kind)
            try container.encode(v, forKey: .data)
        case .subagentSpawned(let v):
            try container.encode("SubagentSpawned", forKey: .kind)
            try container.encode(v, forKey: .data)
        case .subagentCompleted(let v):
            try container.encode("SubagentCompleted", forKey: .kind)
            try container.encode(v, forKey: .data)
        case .turnEnd(let v):
            try container.encode("TurnEnd", forKey: .kind)
            try container.encode(v, forKey: .data)
        case .sessionStatus(let v):
            try container.encode("SessionStatus", forKey: .kind)
            try container.encode(v, forKey: .data)
        }
    }
}

// MARK: - Event Data Types

struct TextChunk: Codable {
    let messageId: String
    let text: String
    let isContinuation: Bool

    enum CodingKeys: String, CodingKey {
        case messageId = "message_id"
        case text
        case isContinuation = "is_continuation"
    }
}

struct ThinkingChunk: Codable {
    let messageId: String
    let text: String
    let isContinuation: Bool

    enum CodingKeys: String, CodingKey {
        case messageId = "message_id"
        case text
        case isContinuation = "is_continuation"
    }
}

struct ToolCallStart: Codable {
    let toolCallId: String
    let toolName: String
    let title: String
    let toolKind: String
    let input: [String: AnyCodable]?

    enum CodingKeys: String, CodingKey {
        case toolCallId = "tool_call_id"
        case toolName = "tool_name"
        case title
        case toolKind = "tool_kind"
        case input
    }
}

struct ToolCallUpdate: Codable {
    let toolCallId: String
    let status: String
    let content: ToolContent?

    enum CodingKeys: String, CodingKey {
        case toolCallId = "tool_call_id"
        case status
        case content
    }
}

enum ToolContent: Codable {
    case text(String)
    case diff(DiffContent)
    case terminal(TerminalContent)

    enum CodingKeys: String, CodingKey {
        case type_ = "type"
        case value
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type_ = try container.decode(String.self, forKey: .type_)
        switch type_ {
        case "Diff":
            self = .diff(try container.decode(DiffContent.self, forKey: .value))
        case "Terminal":
            self = .terminal(try container.decode(TerminalContent.self, forKey: .value))
        default:
            self = .text(try container.decode(String.self, forKey: .value))
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .text(let v):
            try container.encode("Text", forKey: .type_)
            try container.encode(v, forKey: .value)
        case .diff(let v):
            try container.encode("Diff", forKey: .type_)
            try container.encode(v, forKey: .value)
        case .terminal(let v):
            try container.encode("Terminal", forKey: .type_)
            try container.encode(v, forKey: .value)
        }
    }
}

struct DiffContent: Codable {
    let path: String
    let oldText: String
    let newText: String

    enum CodingKeys: String, CodingKey {
        case path
        case oldText = "old_text"
        case newText = "new_text"
    }
}

struct TerminalContent: Codable {
    let terminalId: String
    let output: String
    let exitCode: Int?

    enum CodingKeys: String, CodingKey {
        case terminalId = "terminal_id"
        case output
        case exitCode = "exit_code"
    }
}

struct Plan: Codable {
    let entries: [PlanEntry]
}

struct PlanEntry: Codable, Identifiable {
    let content: String
    let priority: String
    let status: String

    var id: String { content }
}

struct PermissionRequest: Codable {
    let requestId: UInt64
    let toolCallId: String
    let toolName: String
    let description: String
    let options: [PermissionOption]

    enum CodingKeys: String, CodingKey {
        case requestId = "request_id"
        case toolCallId = "tool_call_id"
        case toolName = "tool_name"
        case description
        case options
    }
}

struct PermissionOption: Codable, Identifiable {
    let optionId: String
    let name: String
    let kind: String

    var id: String { optionId }

    enum CodingKeys: String, CodingKey {
        case optionId = "option_id"
        case name
        case kind
    }
}

struct UsageUpdate: Codable {
    let inputTokens: UInt64
    let outputTokens: UInt64
    let cacheReadTokens: UInt64
    let cacheWriteTokens: UInt64
    let costUsd: Double?

    enum CodingKeys: String, CodingKey {
        case inputTokens = "input_tokens"
        case outputTokens = "output_tokens"
        case cacheReadTokens = "cache_read_tokens"
        case cacheWriteTokens = "cache_write_tokens"
        case costUsd = "cost_usd"
    }
}

struct SubagentSpawned: Codable {
    let subagentSessionId: String
    let task: String

    enum CodingKeys: String, CodingKey {
        case subagentSessionId = "subagent_session_id"
        case task
    }
}

struct SubagentCompleted: Codable {
    let subagentSessionId: String
    let success: Bool

    enum CodingKeys: String, CodingKey {
        case subagentSessionId = "subagent_session_id"
        case success
    }
}

struct TurnEnd: Codable {
    let stopReason: String

    enum CodingKeys: String, CodingKey {
        case stopReason = "stop_reason"
    }
}

enum SessionStatus: String, Codable {
    case initializing, ready, working, waitingPermission = "waiting_permission"
    case compacting, idle, terminated
}

// MARK: - Helper for arbitrary JSON

struct AnyCodable: Codable {
    let value: Any

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let str = try? container.decode(String.self) { value = str }
        else if let num = try? container.decode(Double.self) { value = num }
        else if let bool = try? container.decode(Bool.self) { value = bool }
        else { value = "" }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        if let str = value as? String { try container.encode(str) }
        else if let num = value as? Double { try container.encode(num) }
        else if let bool = value as? Bool { try container.encode(bool) }
        else { try container.encodeNil() }
    }
}
