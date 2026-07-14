import SwiftUI

// MARK: - Activity Feed Item

/// A processed, displayable item in the activity feed.
/// Built from raw AgentEvents by the session model.
@Observable
final class ActivityItem: Identifiable {
    let id: String
    var kind: ActivityItemKind
    var timestamp: Date

    init(id: String, kind: ActivityItemKind, timestamp: Date = .now) {
        self.id = id
        self.kind = kind
        self.timestamp = timestamp
    }
}

enum ActivityItemKind {
    case message(text: String)
    case thinking(text: String, isExpanded: Bool)
    case toolCall(ToolCallItem)
    case diff(DiffItem)
    case plan(entries: [PlanEntry])
    case permission(PermissionRequest)
    case subagent(SubagentItem)
    case turnEnd(reason: String)
}

struct ToolCallItem {
    let toolCallId: String
    let toolName: String
    let title: String
    let kind: String
    var status: ToolCallStatus
    var output: String?
    var duration: TimeInterval?
    var isExpanded: Bool

    init(start: ToolCallStart) {
        self.toolCallId = start.toolCallId
        self.toolName = start.toolName
        self.title = start.title
        self.kind = start.toolKind
        self.status = .running
        self.output = nil
        self.duration = nil
        self.isExpanded = false
    }
}

enum ToolCallStatus {
    case pending, running, completed, failed

    var icon: String {
        switch self {
        case .pending: "clock"
        case .running: "arrow.trianglehead.2.clockwise"
        case .completed: "checkmark.circle.fill"
        case .failed: "xmark.circle.fill"
        }
    }

    var color: Color {
        switch self {
        case .pending: DS.text.tertiary
        case .running: DS.accent.primary
        case .completed: DS.status.success
        case .failed: DS.status.error
        }
    }
}

struct DiffItem {
    let path: String
    let oldText: String
    let newText: String
    var isExpanded: Bool

    var additions: Int {
        newText.split(separator: "\n").count - oldText.split(separator: "\n").count
    }
}

struct SubagentItem {
    let sessionId: String
    let task: String
    var isCompleted: Bool
    var success: Bool
}

// MARK: - ThinkingBlock

struct ThinkingBlock: View {
    let text: String
    @State private var isExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: DS.spacing.xs) {
            Button {
                withAnimation(.easeInOut(duration: 0.2)) { isExpanded.toggle() }
            } label: {
                HStack(spacing: DS.spacing.sm) {
                    Image(systemName: "sparkles")
                        .foregroundStyle(DS.accent.primary)
                        .font(.caption)

                    Text("Thinking")
                        .font(.atlasCaption)
                        .foregroundStyle(DS.text.secondary)

                    Spacer()

                    Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                        .font(.caption2)
                        .foregroundStyle(DS.text.tertiary)
                }
            }
            .buttonStyle(.plain)

            if isExpanded {
                Text(text)
                    .font(.atlasCaption)
                    .foregroundStyle(DS.text.tertiary)
                    .lineLimit(nil)
                    .textSelection(.enabled)
                    .padding(DS.spacing.sm)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(DS.bg.base.opacity(0.5))
                    .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm))
            } else if !text.isEmpty {
                Text(text.prefix(120) + (text.count > 120 ? "…" : ""))
                    .font(.atlasCaption)
                    .foregroundStyle(DS.text.tertiary)
                    .lineLimit(1)
            }
        }
        .padding(DS.spacing.sm)
        .background(DS.bg.elevated.opacity(0.3))
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm))
    }
}

// MARK: - ToolCallCard

struct ToolCallCard: View {
    let item: ToolCallItem

    var body: some View {
        VStack(alignment: .leading, spacing: DS.spacing.xs) {
            HStack(spacing: DS.spacing.sm) {
                toolIcon
                    .frame(width: 20, height: 20)

                VStack(alignment: .leading, spacing: 2) {
                    Text(item.title.isEmpty ? item.toolName : item.title)
                        .font(.atlasCaption)
                        .fontWeight(.medium)
                        .foregroundStyle(DS.text.primary)
                        .lineLimit(1)
                }

                Spacer()

                statusBadge
            }

            if let output = item.output, item.isExpanded {
                Text(output)
                    .font(.atlasMono)
                    .foregroundStyle(DS.text.secondary)
                    .lineLimit(20)
                    .textSelection(.enabled)
                    .padding(DS.spacing.sm)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(DS.bg.base)
                    .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm))
            }
        }
        .padding(DS.spacing.sm)
        .background(DS.bg.elevated)
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.md))
        .overlay(
            RoundedRectangle(cornerRadius: DS.radius.md)
                .stroke(DS.border.subtle, lineWidth: 1)
        )
    }

    @ViewBuilder
    private var toolIcon: some View {
        let (icon, color) = iconForKind(item.kind)
        Image(systemName: icon)
            .font(.caption)
            .foregroundStyle(color)
    }

    @ViewBuilder
    private var statusBadge: some View {
        HStack(spacing: 4) {
            if item.status == .running {
                ProgressView()
                    .controlSize(.mini)
            } else {
                Image(systemName: item.status.icon)
                    .font(.caption2)
                    .foregroundStyle(item.status.color)
            }

            if let duration = item.duration {
                Text(String(format: "%.1fs", duration))
                    .font(.atlasCaption)
                    .foregroundStyle(DS.text.tertiary)
            }
        }
    }

    private func iconForKind(_ kind: String) -> (String, Color) {
        switch kind {
        case "read": ("doc.text", DS.accent.primary)
        case "edit": ("pencil", DS.status.warning)
        case "write": ("plus.doc", DS.status.success)
        case "delete": ("trash", DS.status.error)
        case "search", "glob": ("magnifyingglass", DS.accent.primary)
        case "execute": ("terminal", DS.text.secondary)
        case "think": ("brain", DS.accent.primary)
        case "fetch": ("globe", DS.accent.primary)
        default: ("wrench", DS.text.secondary)
        }
    }
}

// MARK: - DiffView

struct DiffBlock: View {
    let item: DiffItem

    var body: some View {
        VStack(alignment: .leading, spacing: DS.spacing.xs) {
            HStack(spacing: DS.spacing.sm) {
                Image(systemName: "doc.badge.gearshape")
                    .font(.caption)
                    .foregroundStyle(DS.status.warning)

                Text(shortenPath(item.path))
                    .font(.atlasCaption)
                    .fontWeight(.medium)
                    .foregroundStyle(DS.text.primary)

                Spacer()

                HStack(spacing: 4) {
                    Text("+\(max(item.additions, 0))")
                        .font(.atlasCaption)
                        .foregroundStyle(DS.status.success)
                }
            }

            if item.isExpanded {
                diffLines
            }
        }
        .padding(DS.spacing.sm)
        .background(DS.bg.elevated)
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.md))
        .overlay(
            RoundedRectangle(cornerRadius: DS.radius.md)
                .stroke(DS.border.subtle, lineWidth: 1)
        )
    }

    @ViewBuilder
    private var diffLines: some View {
        let lines = computeDiffLines()
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(lines.prefix(50).enumerated()), id: \.offset) { _, line in
                HStack(spacing: 0) {
                    Text(line.prefix)
                        .font(.atlasMono)
                        .foregroundStyle(line.color)
                        .frame(width: 14, alignment: .center)

                    Text(line.text)
                        .font(.atlasMono)
                        .foregroundStyle(line.color.opacity(0.8))
                        .lineLimit(1)
                }
                .padding(.horizontal, DS.spacing.xs)
                .background(line.bg)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm))
    }

    private struct DiffLine {
        let prefix: String
        let text: String
        let color: Color
        let bg: Color
    }

    private func computeDiffLines() -> [DiffLine] {
        let oldLines = item.oldText.split(separator: "\n", omittingEmptySubsequences: false)
        let newLines = item.newText.split(separator: "\n", omittingEmptySubsequences: false)

        var result: [DiffLine] = []
        // Simple diff: show removed then added
        for line in oldLines where !newLines.contains(line) {
            result.append(DiffLine(
                prefix: "-",
                text: String(line),
                color: DS.status.error,
                bg: DS.status.error.opacity(0.08)
            ))
        }
        for line in newLines where !oldLines.contains(line) {
            result.append(DiffLine(
                prefix: "+",
                text: String(line),
                color: DS.status.success,
                bg: DS.status.success.opacity(0.08)
            ))
        }
        return result
    }

    private func shortenPath(_ path: String) -> String {
        let components = path.split(separator: "/")
        if components.count > 3 {
            return "…/" + components.suffix(2).joined(separator: "/")
        }
        return path
    }
}

// MARK: - SubagentCard

struct SubagentCard: View {
    let item: SubagentItem

    var body: some View {
        HStack(spacing: DS.spacing.sm) {
            Image(systemName: "point.3.connected.trianglepath.dotted")
                .font(.caption)
                .foregroundStyle(DS.accent.primary)

            VStack(alignment: .leading, spacing: 2) {
                Text("Subagent")
                    .font(.atlasCaption)
                    .foregroundStyle(DS.text.secondary)

                Text(item.task)
                    .font(.atlasCaption)
                    .fontWeight(.medium)
                    .foregroundStyle(DS.text.primary)
                    .lineLimit(2)
            }

            Spacer()

            if item.isCompleted {
                Image(systemName: item.success ? "checkmark.circle.fill" : "xmark.circle.fill")
                    .foregroundStyle(item.success ? DS.status.success : DS.status.error)
            } else {
                ProgressView()
                    .controlSize(.mini)
            }
        }
        .padding(DS.spacing.sm)
        .background(DS.accent.subtle)
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.md))
        .overlay(
            RoundedRectangle(cornerRadius: DS.radius.md)
                .stroke(DS.accent.primary.opacity(0.2), lineWidth: 1)
        )
    }
}

// MARK: - PermissionCard

struct PermissionCard: View {
    let request: PermissionRequest
    let onRespond: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: DS.spacing.md) {
            HStack(spacing: DS.spacing.sm) {
                Image(systemName: "lock.shield")
                    .foregroundStyle(DS.status.warning)

                Text("Permission Required")
                    .font(.atlasCaption)
                    .fontWeight(.semibold)
                    .foregroundStyle(DS.text.primary)
            }

            Text(request.description)
                .font(.atlasCaption)
                .foregroundStyle(DS.text.secondary)

            HStack(spacing: DS.spacing.sm) {
                ForEach(request.options) { option in
                    Button {
                        onRespond(option.optionId)
                    } label: {
                        Text(option.name)
                            .font(.atlasCaption)
                            .fontWeight(.medium)
                            .padding(.horizontal, DS.spacing.md)
                            .padding(.vertical, DS.spacing.sm)
                            .background(bgForOption(option.kind))
                            .foregroundStyle(fgForOption(option.kind))
                            .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm))
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .padding(DS.spacing.md)
        .background(DS.status.warning.opacity(0.08))
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.md))
        .overlay(
            RoundedRectangle(cornerRadius: DS.radius.md)
                .stroke(DS.status.warning.opacity(0.3), lineWidth: 1)
        )
    }

    private func bgForOption(_ kind: String) -> Color {
        switch kind {
        case "allow_once", "allow_session", "allow_always":
            DS.status.success.opacity(0.15)
        case "reject_once", "reject_always":
            DS.status.error.opacity(0.15)
        default:
            DS.bg.elevated2
        }
    }

    private func fgForOption(_ kind: String) -> Color {
        switch kind {
        case "allow_once", "allow_session", "allow_always":
            DS.status.success
        case "reject_once", "reject_always":
            DS.status.error
        default:
            DS.text.primary
        }
    }
}

// MARK: - PlanView

struct PlanView: View {
    let entries: [PlanEntry]

    var body: some View {
        VStack(alignment: .leading, spacing: DS.spacing.xs) {
            HStack(spacing: DS.spacing.sm) {
                Image(systemName: "checklist")
                    .font(.caption)
                    .foregroundStyle(DS.accent.primary)

                Text("Plan")
                    .font(.atlasCaption)
                    .fontWeight(.medium)
                    .foregroundStyle(DS.text.secondary)
            }

            ForEach(entries) { entry in
                HStack(spacing: DS.spacing.sm) {
                    Image(systemName: statusIcon(entry.status))
                        .font(.caption2)
                        .foregroundStyle(statusColor(entry.status))

                    Text(entry.content)
                        .font(.atlasCaption)
                        .foregroundStyle(
                            entry.status == "completed" ? DS.text.tertiary : DS.text.primary
                        )
                        .strikethrough(entry.status == "completed")
                }
            }
        }
        .padding(DS.spacing.sm)
        .background(DS.bg.elevated.opacity(0.5))
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm))
    }

    private func statusIcon(_ status: String) -> String {
        switch status {
        case "completed": "checkmark.circle.fill"
        case "in_progress": "circle.dotted"
        case "skipped": "minus.circle"
        default: "circle"
        }
    }

    private func statusColor(_ status: String) -> Color {
        switch status {
        case "completed": DS.status.success
        case "in_progress": DS.accent.primary
        case "skipped": DS.text.tertiary
        default: DS.text.tertiary
        }
    }
}

// MARK: - MessageBlock

struct MessageBlock: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.atlasBody)
            .foregroundStyle(DS.text.primary)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}
