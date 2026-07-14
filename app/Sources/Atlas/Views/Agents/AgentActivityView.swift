import SwiftUI

// MARK: - Agent Activity Session Model

@Observable
final class AgentActivitySession {
    let sessionId: String
    let adapterName: String
    var items: [ActivityItem] = []
    var isWorking = false
    var currentOperation: String = ""
    var usageTokens: UInt64 = 0
    var costUsd: Double = 0
    var elapsedSeconds: Int = 0
    var pendingPermission: PermissionRequest?
    private var startTime = Date()
    private var timer: Timer?
    private var lastMessageId: String?

    init(sessionId: String, adapterName: String) {
        self.sessionId = sessionId
        self.adapterName = adapterName
        startTimer()
    }

    func apply(event: AgentEvent) {
        switch event.event {
        case .textChunk(let chunk):
            if chunk.isContinuation, let last = items.last,
               case .message(let existing) = last.kind {
                last.kind = .message(text: existing + chunk.text)
            } else {
                items.append(ActivityItem(
                    id: "msg-\(chunk.messageId)-\(items.count)",
                    kind: .message(text: chunk.text)
                ))
            }
            lastMessageId = chunk.messageId

        case .thinkingChunk(let chunk):
            if chunk.isContinuation, let last = items.last,
               case .thinking(let existing, let exp) = last.kind {
                last.kind = .thinking(text: existing + chunk.text, isExpanded: exp)
            } else {
                items.append(ActivityItem(
                    id: "think-\(items.count)",
                    kind: .thinking(text: chunk.text, isExpanded: false)
                ))
            }

        case .toolCallStart(let start):
            isWorking = true
            currentOperation = start.title.isEmpty ? start.toolName : start.title
            items.append(ActivityItem(
                id: "tool-\(start.toolCallId)",
                kind: .toolCall(ToolCallItem(start: start))
            ))

        case .toolCallUpdate(let update):
            if let item = items.first(where: { item in
                if case .toolCall(let tc) = item.kind { return tc.toolCallId == update.toolCallId }
                return false
            }), case .toolCall(var tc) = item.kind {
                tc.status = statusFromString(update.status)
                if let content = update.content {
                    switch content {
                    case .text(let t): tc.output = t
                    case .diff(let d):
                        // Add a separate diff item
                        items.append(ActivityItem(
                            id: "diff-\(update.toolCallId)",
                            kind: .diff(DiffItem(path: d.path, oldText: d.oldText, newText: d.newText, isExpanded: false))
                        ))
                    case .terminal(let t): tc.output = t.output
                    }
                }
                if tc.status == .completed || tc.status == .failed {
                    isWorking = items.contains { item in
                        if case .toolCall(let tc2) = item.kind { return tc2.status == .running }
                        return false
                    }
                }
                item.kind = .toolCall(tc)
            }

        case .plan(let plan):
            items.append(ActivityItem(
                id: "plan-\(items.count)",
                kind: .plan(entries: plan.entries)
            ))

        case .permissionRequest(let req):
            pendingPermission = req
            items.append(ActivityItem(
                id: "perm-\(req.requestId)",
                kind: .permission(req)
            ))

        case .usageUpdate(let usage):
            usageTokens = usage.inputTokens + usage.outputTokens
            if let cost = usage.costUsd {
                costUsd = cost
            }

        case .subagentSpawned(let sub):
            items.append(ActivityItem(
                id: "sub-\(sub.subagentSessionId)",
                kind: .subagent(SubagentItem(
                    sessionId: sub.subagentSessionId,
                    task: sub.task,
                    isCompleted: false,
                    success: false
                ))
            ))

        case .subagentCompleted(let sub):
            if let item = items.first(where: { item in
                if case .subagent(let s) = item.kind { return s.sessionId == sub.subagentSessionId }
                return false
            }), case .subagent(var s) = item.kind {
                s.isCompleted = true
                s.success = sub.success
                item.kind = .subagent(s)
            }

        case .turnEnd(let end):
            isWorking = false
            currentOperation = ""

        case .sessionStatus(let status):
            isWorking = (status == .working)
        }
    }

    private func startTimer() {
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.elapsedSeconds = Int(Date().timeIntervalSince(self.startTime))
        }
    }

    private func statusFromString(_ s: String) -> ToolCallStatus {
        switch s {
        case "pending": .pending
        case "in_progress": .running
        case "completed": .completed
        case "failed": .failed
        default: .running
        }
    }

    deinit {
        timer?.invalidate()
    }
}

// MARK: - AgentActivityView

struct AgentActivityView: View {
    @State var session: AgentActivitySession
    var onSendMessage: (String) -> Void
    var onPermissionRespond: (UInt64, String) -> Void
    @State private var inputText = ""
    @State private var autoScroll = true

    var body: some View {
        VStack(spacing: 0) {
            // Header
            header

            SoftDivider()

            // Activity Feed
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: DS.spacing.md) {
                        ForEach(session.items) { item in
                            activityItemView(item)
                                .id(item.id)
                        }
                    }
                    .padding(DS.spacing.lg)
                }
                .onChange(of: session.items.count) { _, _ in
                    if autoScroll, let last = session.items.last {
                        withAnimation(.easeOut(duration: 0.2)) {
                            proxy.scrollTo(last.id, anchor: .bottom)
                        }
                    }
                }
            }

            // Progress bar (pinned at bottom when working)
            if session.isWorking {
                progressBar
            }

            SoftDivider()

            // Input
            inputBar
        }
        .background(DS.bg.base)
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: DS.spacing.md) {
            HStack(spacing: DS.spacing.sm) {
                Circle()
                    .fill(session.isWorking ? DS.accent.primary : DS.status.success)
                    .frame(width: 8, height: 8)
                    .overlay {
                        if session.isWorking {
                            Circle()
                                .stroke(DS.accent.primary, lineWidth: 2)
                                .frame(width: 14, height: 14)
                                .opacity(0.5)
                                .scaleEffect(session.isWorking ? 1.5 : 1)
                                .animation(
                                    .easeInOut(duration: 1).repeatForever(autoreverses: true),
                                    value: session.isWorking
                                )
                        }
                    }

                Text(session.adapterName.capitalized)
                    .font(.atlasHeadline)
                    .foregroundStyle(DS.text.primary)
            }

            Spacer()

            HStack(spacing: DS.spacing.lg) {
                if session.usageTokens > 0 {
                    Label("\(session.usageTokens / 1000)k", systemImage: "brain")
                        .font(.atlasCaption)
                        .foregroundStyle(DS.text.tertiary)
                }

                if session.costUsd > 0 {
                    Text(String(format: "$%.3f", session.costUsd))
                        .font(.atlasCaption)
                        .foregroundStyle(DS.text.tertiary)
                }

                Text(formatElapsed(session.elapsedSeconds))
                    .font(.atlasMono)
                    .foregroundStyle(DS.text.tertiary)
            }
        }
        .padding(.horizontal, DS.spacing.lg)
        .padding(.vertical, DS.spacing.md)
    }

    // MARK: - Progress Bar

    private var progressBar: some View {
        HStack(spacing: DS.spacing.sm) {
            ProgressView()
                .controlSize(.small)
                .tint(DS.accent.primary)

            Text(session.currentOperation)
                .font(.atlasCaption)
                .foregroundStyle(DS.text.secondary)
                .lineLimit(1)

            Spacer()
        }
        .padding(.horizontal, DS.spacing.lg)
        .padding(.vertical, DS.spacing.sm)
        .background(DS.bg.elevated)
    }

    // MARK: - Input Bar

    private var inputBar: some View {
        HStack(spacing: DS.spacing.sm) {
            TextField("Ask the agent...", text: $inputText)
                .textFieldStyle(.plain)
                .font(.atlasBody)
                .padding(DS.spacing.md)
                .background(DS.bg.elevated)
                .clipShape(RoundedRectangle(cornerRadius: DS.radius.md))
                .onSubmit { sendMessage() }

            Button(action: sendMessage) {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.title2)
                    .foregroundStyle(
                        inputText.isEmpty ? DS.text.disabled : DS.accent.primary
                    )
            }
            .buttonStyle(.plain)
            .disabled(inputText.isEmpty)
        }
        .padding(DS.spacing.md)
    }

    // MARK: - Item Routing

    @ViewBuilder
    private func activityItemView(_ item: ActivityItem) -> some View {
        switch item.kind {
        case .message(let text):
            MessageBlock(text: text)

        case .thinking(let text, _):
            ThinkingBlock(text: text)

        case .toolCall(let tc):
            ToolCallCard(item: tc)

        case .diff(let d):
            DiffBlock(item: d)

        case .plan(let entries):
            PlanView(entries: entries)

        case .permission(let req):
            PermissionCard(request: req) { optionId in
                onPermissionRespond(req.requestId, optionId)
            }

        case .subagent(let sub):
            SubagentCard(item: sub)

        case .turnEnd:
            HStack {
                Rectangle()
                    .fill(DS.border.subtle)
                    .frame(height: 1)
                Text("Done")
                    .font(.atlasCaption)
                    .foregroundStyle(DS.text.tertiary)
                Rectangle()
                    .fill(DS.border.subtle)
                    .frame(height: 1)
            }
        }
    }

    // MARK: - Helpers

    private func sendMessage() {
        guard !inputText.isEmpty else { return }
        let msg = inputText
        inputText = ""
        onSendMessage(msg)
    }

    private func formatElapsed(_ seconds: Int) -> String {
        let m = seconds / 60
        let s = seconds % 60
        return String(format: "%d:%02d", m, s)
    }
}
