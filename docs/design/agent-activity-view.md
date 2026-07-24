# AgentActivityView — Hybrid UI Design Spec

## Philosophy

A terminal shows you text. A hybrid activity view shows you **what the agent is doing, why, and what it needs from you**. The goal is a native macOS experience that gives the developer:

1. **Glanceable progress** — know the status in < 1 second
2. **Deep inspectability** — drill into any step without leaving the view
3. **Agency** — clearly see when input is needed vs. when to let it run
4. **Accountability** — every action is traceable, undoable, reviewable

---

## Design Decisions

### Why NOT pure chat bubbles

Chat bubbles (à la ChatGPT) break down for agent output because:
- Tool calls and file edits aren't "messages" — they're **actions**
- Subagents create hierarchy that flat chat can't represent
- Progress state needs persistent visibility, not scroll-away text
- Diffs, code blocks, and thinking need distinct visual treatment

### Why NOT pure terminal

Terminal is raw text with no structure. Developers lose:
- Ability to collapse verbose thinking
- Clickable file paths and interactive diffs
- Visual distinction between tool calls, code output, and prose
- Progress state (you have to watch the cursor)

### The Hybrid: Activity Feed with Smart Blocks

Inspired by:
- **Linear** — clean activity timeline, collapsible details, status-first design
- **Raycast** — native macOS feel, command-result pairs, instant response
- **Arc Browser** — progressive disclosure, minimal chrome, focus on content
- **CleanMyMac** — animated progress with clear phase transitions, visual metaphors for state

The model is: **a vertical activity feed of typed blocks**, where each block is a rich, interactive card appropriate to its content type. The feed scrolls, but the current activity is always pinned/visible at the bottom.

---

## Interaction Model

### Three Modes (mutually exclusive)

| Mode | Visual State | User Action |
|------|-------------|-------------|
| **Working** | Progress indicator pinned at bottom, blocks stream in | Watch, scroll history |
| **Waiting for Input** | Input bar appears with context, progress pauses | Type response, approve action |
| **Complete** | Summary card at bottom, full history scrollable | Review, copy, retry |

### Progressive Disclosure

Every block has two states:
- **Collapsed** — 1-2 lines: icon + title + status badge
- **Expanded** — full content: params, output, diff, etc.

Default behavior:
- ThinkingBlock: collapsed (expandable)
- ToolCallCard: collapsed while running, auto-expand on completion if has meaningful output
- CodeBlock: always visible (it's the point)
- DiffView: collapsed showing file name + stats, expandable to full diff
- SubagentCard: collapsed showing name + status

---

## Component Hierarchy

```
AgentActivityView (main container)
├── ActivityHeader
│   ├── AgentIdentity (model name, session ID)
│   ├── SessionControls (stop, pause, retry)
│   └── ProgressSummary (elapsed time, steps completed)
│
├── ActivityFeed (scrollable, LazyVStack)
│   ├── ThinkingBlock
│   ├── ToolCallCard
│   │   └── ToolOutputView (nested: CodeBlock | DiffView | TextBlock)
│   ├── CodeBlock
│   ├── DiffView
│   ├── SubagentCard
│   │   └── ActivityFeed (recursive, nested)
│   ├── TextBlock (prose output, markdown)
│   └── ErrorBlock
│
├── LiveProgressBar (pinned, shows current operation)
│
└── InputArea (contextual, appears when agent needs input)
    ├── PromptField
    ├── ApprovalButtons (approve/reject for confirmations)
    └── ContextHint (what the agent is asking about)
```

---

## Component Specifications

### 1. AgentActivityView

The root container. Manages the feed, live progress, and input area.

```swift
import SwiftUI

struct AgentActivityView: View {
    let session: AgentSession
    @Environment(AppState.self) private var appState
    @State private var autoScroll = true

    var body: some View {
        VStack(spacing: 0) {
            ActivityHeader(session: session)
            
            SoftDivider()

            ZStack(alignment: .bottom) {
                ActivityFeed(
                    blocks: session.blocks,
                    autoScroll: $autoScroll
                )

                if session.isWorking {
                    LiveProgressBar(activity: session.currentActivity)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }

            SoftDivider()

            InputArea(
                mode: session.inputMode,
                onSubmit: { text in
                    Task { await appState.sendPromptToAgent(sessionId: session.id, prompt: text) }
                },
                onApprove: {
                    Task { await appState.approveAgentAction(sessionId: session.id) }
                },
                onReject: {
                    Task { await appState.rejectAgentAction(sessionId: session.id) }
                }
            )
        }
        .background(DS.bg.base)
    }
}
```

**Layout:**
- Full height of the detail pane
- No horizontal padding on container (blocks manage their own)
- Background: `DS.bg.base`

---

### 2. ActivityHeader

Persistent top bar showing session identity and controls.

```swift
struct ActivityHeader: View {
    let session: AgentSession
    @Environment(AppState.self) private var appState

    var body: some View {
        HStack(spacing: DS.spacing.md) {
            // Agent identity
            HStack(spacing: DS.spacing.sm) {
                Image(systemName: "cpu")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(DS.accent.primary)

                Text(session.modelName)
                    .font(.atlasHeadline)
                    .foregroundStyle(DS.text.primary)
            }

            Spacer()

            // Progress summary
            HStack(spacing: DS.spacing.sm) {
                Text(session.elapsedFormatted)
                    .font(.atlasCaption)
                    .foregroundStyle(DS.text.tertiary)
                    .monospacedDigit()

                Text("·")
                    .foregroundStyle(DS.text.tertiary)

                Text("\(session.completedSteps) steps")
                    .font(.atlasCaption)
                    .foregroundStyle(DS.text.tertiary)
            }

            // Controls
            HStack(spacing: DS.spacing.sm) {
                Button {
                    Task { await appState.stopAgent(sessionId: session.id) }
                } label: {
                    Image(systemName: "stop.fill")
                        .font(.system(size: 11))
                }
                .buttonStyle(.plain)
                .foregroundStyle(DS.status.error)
                .help("Stop agent")
            }
        }
        .padding(.horizontal, DS.spacing.lg)
        .padding(.vertical, DS.spacing.md)
    }
}
```

---

### 3. ActivityFeed

The scrollable container for all activity blocks.

```swift
struct ActivityFeed: View {
    let blocks: [ActivityBlock]
    @Binding var autoScroll: Bool

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: DS.spacing.xs) {
                    ForEach(blocks) { block in
                        ActivityBlockView(block: block)
                            .id(block.id)
                            .padding(.horizontal, DS.spacing.lg)
                    }
                }
                .padding(.vertical, DS.spacing.md)
            }
            .onChange(of: blocks.count) {
                guard autoScroll, let last = blocks.last else { return }
                withAnimation(.easeOut(duration: 0.15)) {
                    proxy.scrollTo(last.id, anchor: .bottom)
                }
            }
            .onScrollPhaseChange { _, newPhase in
                // Disable auto-scroll when user scrolls up
                autoScroll = (newPhase == .idle)
            }
        }
    }
}
```

**Key decisions:**
- `LazyVStack` for performance with many blocks
- `spacing: DS.spacing.xs` (4pt) — blocks are visually separated by their own internal padding and surface treatment, not by large gaps
- Auto-scroll disabled when user manually scrolls (like terminal behavior)

---

### 4. ThinkingBlock

Collapsible block for agent reasoning. Shows a preview line when collapsed.

```swift
struct ThinkingBlock: View {
    let thinking: ThinkingContent
    @State private var isExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header (always visible, tappable)
            Button { withAnimation(.snappy(duration: 0.2)) { isExpanded.toggle() } } label: {
                HStack(spacing: DS.spacing.sm) {
                    Image(systemName: "sparkle")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(DS.text.tertiary)

                    Text("Thinking")
                        .font(.atlasCaption)
                        .foregroundStyle(DS.text.tertiary)

                    if !isExpanded {
                        Text(thinking.preview)
                            .font(.atlasCaption)
                            .foregroundStyle(DS.text.tertiary)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }

                    Spacer()

                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(DS.text.tertiary)
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                }
                .padding(.horizontal, DS.spacing.md)
                .padding(.vertical, DS.spacing.sm)
            }
            .buttonStyle(.plain)

            // Expanded content
            if isExpanded {
                Text(thinking.fullText)
                    .font(.atlasBody)
                    .foregroundStyle(DS.text.secondary)
                    .padding(.horizontal, DS.spacing.md)
                    .padding(.bottom, DS.spacing.sm)
                    .textSelection(.enabled)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .background(DS.bg.base)
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm, style: .continuous))
    }
}
```

**Visual treatment:**
- No elevated surface — sits flush with background
- Subtle sparkle icon to indicate "internal reasoning"
- Collapsed shows first ~60 chars as preview
- Expanded: secondary text color, full content
- Animation: `.snappy(duration: 0.2)` for the chevron rotation + content reveal

---

### 5. ToolCallCard

The most important block. Shows a tool invocation with its lifecycle.

```swift
struct ToolCallCard: View {
    let toolCall: ToolCallContent
    @State private var isExpanded: Bool

    init(toolCall: ToolCallContent) {
        self.toolCall = toolCall
        self._isExpanded = State(initialValue: toolCall.status == .completed && toolCall.hasOutput)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            Button { withAnimation(.snappy(duration: 0.2)) { isExpanded.toggle() } } label: {
                HStack(spacing: DS.spacing.sm) {
                    // Tool icon
                    ToolIcon(name: toolCall.toolName)

                    // Tool name + params summary
                    VStack(alignment: .leading, spacing: 2) {
                        Text(toolCall.displayName)
                            .font(.system(.body, design: .monospaced, weight: .medium))
                            .foregroundStyle(DS.text.primary)

                        if let paramsSummary = toolCall.paramsSummary {
                            Text(paramsSummary)
                                .font(.atlasCaption)
                                .foregroundStyle(DS.text.secondary)
                                .lineLimit(1)
                        }
                    }

                    Spacer()

                    // Status
                    ToolStatusIndicator(status: toolCall.status)
                }
                .padding(DS.spacing.md)
            }
            .buttonStyle(.plain)

            // Expanded output
            if isExpanded {
                SoftDivider()
                    .padding(.horizontal, DS.spacing.md)

                ToolOutputView(output: toolCall.output)
                    .padding(DS.spacing.md)
                    .transition(.opacity)
            }
        }
        .background(DS.bg.elevated)
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                .strokeBorder(statusBorderColor, lineWidth: 0.5)
        )
    }

    private var statusBorderColor: Color {
        switch toolCall.status {
        case .running: DS.accent.primary.opacity(0.3)
        case .completed: DS.border.subtle
        case .failed: DS.status.error.opacity(0.3)
        }
    }
}
```

**Tool icon mapping:**

```swift
struct ToolIcon: View {
    let name: String

    var body: some View {
        Image(systemName: iconName)
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(iconColor)
            .frame(width: 24, height: 24)
            .background(iconColor.opacity(0.12))
            .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm, style: .continuous))
    }

    private var iconName: String {
        switch name {
        case "read_file", "read": "doc.text"
        case "write_file", "write", "create_file": "doc.badge.plus"
        case "shell", "bash", "execute": "terminal"
        case "search", "grep", "glob": "magnifyingglass"
        case "web_search": "globe"
        case "web_fetch": "arrow.down.doc"
        case "code": "chevron.left.forwardslash.chevron.right"
        case "use_aws": "cloud"
        case "knowledge": "brain"
        default: "gearshape"
        }
    }

    private var iconColor: Color {
        switch name {
        case "read_file", "read": DS.accent.primary
        case "write_file", "write", "create_file": DS.status.success
        case "shell", "bash", "execute": DS.status.warning
        case "search", "grep", "glob": DS.text.secondary
        case "web_search", "web_fetch": DS.accent.primary
        case "use_aws": DS.status.warning
        default: DS.text.tertiary
        }
    }
}
```

**Status indicator:**

```swift
struct ToolStatusIndicator: View {
    let status: ToolCallStatus

    var body: some View {
        switch status {
        case .running:
            ProgressView()
                .controlSize(.mini)
        case .completed:
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 12))
                .foregroundStyle(DS.status.success)
        case .failed:
            Image(systemName: "xmark.circle.fill")
                .font(.system(size: 12))
                .foregroundStyle(DS.status.error)
        }
    }
}
```

---

### 6. CodeBlock

Syntax-highlighted, copyable code display.

```swift
struct CodeBlock: View {
    let code: CodeContent
    @State private var isCopied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header with language + copy button
            HStack {
                if let language = code.language {
                    Text(language)
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .foregroundStyle(DS.text.tertiary)
                }

                Spacer()

                Button {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(code.content, forType: .string)
                    isCopied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 2) { isCopied = false }
                } label: {
                    Image(systemName: isCopied ? "checkmark" : "doc.on.doc")
                        .font(.system(size: 11))
                        .foregroundStyle(isCopied ? DS.status.success : DS.text.tertiary)
                }
                .buttonStyle(.plain)
                .help("Copy to clipboard")
            }
            .padding(.horizontal, DS.spacing.md)
            .padding(.top, DS.spacing.sm)
            .padding(.bottom, DS.spacing.xs)

            // Code content
            ScrollView(.horizontal, showsIndicators: false) {
                Text(code.highlighted)
                    .font(.system(size: 12, weight: .regular, design: .monospaced))
                    .foregroundStyle(DS.text.primary)
                    .textSelection(.enabled)
                    .padding(.horizontal, DS.spacing.md)
                    .padding(.bottom, DS.spacing.sm)
            }
        }
        .background(DS.bg.base)
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                .strokeBorder(DS.border.subtle, lineWidth: 0.5)
        )
    }
}
```

**Syntax highlighting strategy:**
- Use `AttributedString` with pre-computed highlighting
- Parse on background thread, cache results
- Support: Swift, Rust, TypeScript, Python, YAML, JSON, SQL, Shell
- Color scheme derived from DS tokens (not a third-party theme)

---

### 7. DiffView

Inline diff display with file path and change stats.

```swift
struct DiffView: View {
    let diff: DiffContent
    @State private var isExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header — always visible
            Button { withAnimation(.snappy(duration: 0.2)) { isExpanded.toggle() } } label: {
                HStack(spacing: DS.spacing.sm) {
                    Image(systemName: "doc.text")
                        .font(.system(size: 11))
                        .foregroundStyle(DS.text.secondary)

                    Text(diff.filePath)
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(DS.text.primary)
                        .lineLimit(1)
                        .truncationMode(.middle)

                    Spacer()

                    // Change stats
                    HStack(spacing: DS.spacing.xs) {
                        if diff.additions > 0 {
                            Text("+\(diff.additions)")
                                .font(.system(size: 11, weight: .medium, design: .monospaced))
                                .foregroundStyle(DS.status.success)
                        }
                        if diff.deletions > 0 {
                            Text("-\(diff.deletions)")
                                .font(.system(size: 11, weight: .medium, design: .monospaced))
                                .foregroundStyle(DS.status.error)
                        }
                    }

                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(DS.text.tertiary)
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                }
                .padding(DS.spacing.md)
            }
            .buttonStyle(.plain)

            // Expanded diff content
            if isExpanded {
                SoftDivider()

                ScrollView(.horizontal, showsIndicators: false) {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(diff.lines) { line in
                            DiffLineView(line: line)
                        }
                    }
                    .padding(.vertical, DS.spacing.xs)
                }
                .frame(maxHeight: 400) // Cap height, scrollable
                .transition(.opacity)
            }
        }
        .background(DS.bg.elevated)
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                .strokeBorder(DS.border.subtle, lineWidth: 0.5)
        )
    }
}

struct DiffLineView: View {
    let line: DiffLine

    var body: some View {
        HStack(spacing: 0) {
            // Line number
            Text(line.lineNumber)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(DS.text.tertiary)
                .frame(width: 40, alignment: .trailing)
                .padding(.trailing, DS.spacing.sm)

            // Content
            Text(line.content)
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(lineColor)
                .textSelection(.enabled)
        }
        .padding(.horizontal, DS.spacing.md)
        .padding(.vertical, 1)
        .background(lineBackground)
    }

    private var lineColor: Color {
        switch line.type {
        case .addition: DS.text.primary
        case .deletion: DS.text.primary
        case .context: DS.text.secondary
        }
    }

    private var lineBackground: Color {
        switch line.type {
        case .addition: DS.status.success.opacity(0.08)
        case .deletion: DS.status.error.opacity(0.08)
        case .context: .clear
        }
    }
}
```

---

### 8. SubagentCard

Shows a spawned sub-task with its own nested activity feed.

```swift
struct SubagentCard: View {
    let subagent: SubagentContent
    @State private var isExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            Button { withAnimation(.snappy(duration: 0.2)) { isExpanded.toggle() } } label: {
                HStack(spacing: DS.spacing.sm) {
                    // Nested agent icon
                    Image(systemName: "arrow.triangle.branch")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(DS.accent.primary)
                        .frame(width: 24, height: 24)
                        .background(DS.accent.subtle)
                        .clipShape(RoundedRectangle(cornerRadius: DS.radius.sm, style: .continuous))

                    VStack(alignment: .leading, spacing: 2) {
                        Text(subagent.taskDescription)
                            .font(.atlasBody)
                            .foregroundStyle(DS.text.primary)
                            .lineLimit(1)

                        Text(subagent.role)
                            .font(.atlasCaption)
                            .foregroundStyle(DS.text.tertiary)
                    }

                    Spacer()

                    // Status badge
                    StatusBadge(label: subagent.status.displayName, color: subagent.status.color)

                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(DS.text.tertiary)
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                }
                .padding(DS.spacing.md)
            }
            .buttonStyle(.plain)

            // Nested activity feed
            if isExpanded {
                SoftDivider()

                VStack(alignment: .leading, spacing: DS.spacing.xs) {
                    ForEach(subagent.blocks) { block in
                        ActivityBlockView(block: block)
                            .padding(.leading, DS.spacing.lg) // Indent nested
                    }
                }
                .padding(.vertical, DS.spacing.sm)
                .padding(.horizontal, DS.spacing.md)
                .transition(.opacity)
            }
        }
        .background(DS.bg.elevated)
        .clipShape(RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: DS.radius.md, style: .continuous)
                .strokeBorder(DS.accent.primary.opacity(0.15), lineWidth: 0.5)
        )
    }
}
```

---

### 9. LiveProgressBar

Pinned at the bottom of the feed area. Shows what's happening RIGHT NOW.

```swift
struct LiveProgressBar: View {
    let activity: CurrentActivity

    var body: some View {
        HStack(spacing: DS.spacing.sm) {
            // Animated pulse indicator
            PulseIndicator()

            Text(activity.description)
                .font(.atlasCaption)
                .foregroundStyle(DS.text.secondary)
                .lineLimit(1)

            Spacer()

            if let progress = activity.progress {
                Text(progress)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(DS.text.tertiary)
            }
        }
        .padding(.horizontal, DS.spacing.lg)
        .padding(.vertical, DS.spacing.sm)
        .background(.ultraThinMaterial)
        .overlay(alignment: .top) {
            SoftDivider()
        }
    }
}

struct PulseIndicator: View {
    @State private var isAnimating = false

    var body: some View {
        Circle()
            .fill(DS.accent.primary)
            .frame(width: 6, height: 6)
            .scaleEffect(isAnimating ? 1.3 : 1.0)
            .opacity(isAnimating ? 0.6 : 1.0)
            .animation(
                .easeInOut(duration: 0.8).repeatForever(autoreverses: true),
                value: isAnimating
            )
            .onAppear { isAnimating = true }
    }
}
```

---

### 10. InputArea

Contextual input that adapts to what the agent needs.

```swift
struct InputArea: View {
    let mode: InputMode
    let onSubmit: (String) -> Void
    let onApprove: () -> Void
    let onReject: () -> Void
    @State private var text = ""
    @FocusState private var isFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            switch mode {
            case .prompt(let context):
                promptInput(context: context)

            case .approval(let action):
                approvalInput(action: action)

            case .idle:
                idleInput()
            }
        }
        .background(DS.bg.elevated.opacity(0.5))
    }

    @ViewBuilder
    private func promptInput(context: String?) -> some View {
        if let context {
            HStack(spacing: DS.spacing.sm) {
                Image(systemName: "questionmark.circle")
                    .font(.system(size: 11))
                    .foregroundStyle(DS.status.warning)
                Text(context)
                    .font(.atlasCaption)
                    .foregroundStyle(DS.text.secondary)
                    .lineLimit(1)
            }
            .padding(.horizontal, DS.spacing.lg)
            .padding(.top, DS.spacing.sm)
        }

        HStack(spacing: DS.spacing.sm) {
            TextField("Respond to agent...", text: $text, axis: .vertical)
                .textFieldStyle(.plain)
                .font(.atlasBody)
                .foregroundStyle(DS.text.primary)
                .lineLimit(1...5)
                .focused($isFocused)
                .onSubmit { submit() }

            Button { submit() } label: {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.system(size: 22))
                    .foregroundStyle(
                        text.isEmpty ? DS.text.tertiary : DS.accent.primary
                    )
            }
            .buttonStyle(.plain)
            .disabled(text.isEmpty)
            .keyboardShortcut(.return, modifiers: .command)
        }
        .padding(.horizontal, DS.spacing.lg)
        .padding(.vertical, DS.spacing.md)
    }

    @ViewBuilder
    private func approvalInput(action: PendingAction) -> some View {
        VStack(alignment: .leading, spacing: DS.spacing.sm) {
            HStack(spacing: DS.spacing.sm) {
                Image(systemName: "exclamationmark.shield")
                    .foregroundStyle(DS.status.warning)
                Text("Agent wants to: \(action.description)")
                    .font(.atlasBody)
                    .foregroundStyle(DS.text.primary)
            }

            HStack(spacing: DS.spacing.sm) {
                AtlasButton("Approve", icon: "checkmark", style: .primary) {
                    onApprove()
                }
                AtlasButton("Reject", icon: "xmark", style: .secondary) {
                    onReject()
                }
                Spacer()
            }
        }
        .padding(.horizontal, DS.spacing.lg)
        .padding(.vertical, DS.spacing.md)
    }

    @ViewBuilder
    private func idleInput() -> some View {
        HStack(spacing: DS.spacing.sm) {
            TextField("Send a follow-up...", text: $text, axis: .vertical)
                .textFieldStyle(.plain)
                .font(.atlasBody)
                .foregroundStyle(DS.text.primary)
                .lineLimit(1...5)
                .focused($isFocused)
                .onSubmit { submit() }

            Button { submit() } label: {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.system(size: 22))
                    .foregroundStyle(
                        text.isEmpty ? DS.text.tertiary : DS.accent.primary
                    )
            }
            .buttonStyle(.plain)
            .disabled(text.isEmpty)
        }
        .padding(.horizontal, DS.spacing.lg)
        .padding(.vertical, DS.spacing.md)
    }

    private func submit() {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return }
        text = ""
        onSubmit(trimmed)
    }
}
```

---

### 11. ActivityBlockView (Router)

Dispatches to the correct view based on block type.

```swift
struct ActivityBlockView: View {
    let block: ActivityBlock

    var body: some View {
        switch block.content {
        case .thinking(let content):
            ThinkingBlock(thinking: content)

        case .toolCall(let content):
            ToolCallCard(toolCall: content)

        case .code(let content):
            CodeBlock(code: content)

        case .diff(let content):
            DiffView(diff: content)

        case .subagent(let content):
            SubagentCard(subagent: content)

        case .text(let content):
            TextBlock(text: content)

        case .error(let content):
            ErrorBlock(error: content)
        }
    }
}
```

---

## Data Models

```swift
import Foundation

// MARK: - Session

struct AgentSession: Identifiable {
    let id: String
    let modelName: String
    var blocks: [ActivityBlock]
    var currentActivity: CurrentActivity?
    var inputMode: InputMode
    var startedAt: Date
    var completedSteps: Int

    var isWorking: Bool { currentActivity != nil }
    var elapsedFormatted: String {
        let elapsed = Date().timeIntervalSince(startedAt)
        let minutes = Int(elapsed) / 60
        let seconds = Int(elapsed) % 60
        return String(format: "%d:%02d", minutes, seconds)
    }
}

// MARK: - Block

struct ActivityBlock: Identifiable {
    let id: String
    let timestamp: Date
    let content: BlockContent
}

enum BlockContent {
    case thinking(ThinkingContent)
    case toolCall(ToolCallContent)
    case code(CodeContent)
    case diff(DiffContent)
    case subagent(SubagentContent)
    case text(TextContent)
    case error(ErrorContent)
}

// MARK: - Content Types

struct ThinkingContent {
    let fullText: String
    var preview: String { String(fullText.prefix(80)) + (fullText.count > 80 ? "…" : "") }
}

struct ToolCallContent {
    let toolName: String
    let displayName: String
    let params: [String: String]
    var paramsSummary: String? {
        params.values.first.map { String($0.prefix(60)) }
    }
    var status: ToolCallStatus
    var output: ToolOutput?
    var hasOutput: Bool { output != nil }
}

enum ToolCallStatus {
    case running, completed, failed
}

enum ToolOutput {
    case code(CodeContent)
    case diff(DiffContent)
    case text(String)
    case error(String)
}

struct CodeContent {
    let content: String
    let language: String?
    var highlighted: AttributedString { /* computed via syntax engine */ AttributedString(content) }
}

struct DiffContent {
    let filePath: String
    let lines: [DiffLine]
    var additions: Int { lines.filter { $0.type == .addition }.count }
    var deletions: Int { lines.filter { $0.type == .deletion }.count }
}

struct DiffLine: Identifiable {
    let id: String
    let lineNumber: String
    let content: String
    let type: DiffLineType
}

enum DiffLineType {
    case addition, deletion, context
}

struct SubagentContent {
    let taskDescription: String
    let role: String
    var status: SubagentStatus
    var blocks: [ActivityBlock]
}

enum SubagentStatus {
    case running, completed, failed

    var displayName: String {
        switch self {
        case .running: "Running"
        case .completed: "Done"
        case .failed: "Failed"
        }
    }

    var color: Color {
        switch self {
        case .running: DS.accent.primary
        case .completed: DS.status.success
        case .failed: DS.status.error
        }
    }
}

struct TextContent {
    let markdown: String
}

struct ErrorContent {
    let message: String
    let details: String?
}

// MARK: - Input

enum InputMode {
    case idle
    case prompt(context: String?)
    case approval(action: PendingAction)
}

struct PendingAction {
    let description: String
    let risk: ActionRisk
}

enum ActionRisk { case low, medium, high }

// MARK: - Progress

struct CurrentActivity {
    let description: String
    let progress: String? // e.g., "3/12 files"
}
```

---

## State Transitions

```
                    ┌────────────────────────────────────────┐
                    │                                        │
                    ▼                                        │
┌─────────┐   ┌─────────┐   ┌──────────────────┐   ┌──────┴──────┐
│  Idle   │──▶│ Working │──▶│ Waiting for Input │──▶│  Working    │
│         │   │         │   │                    │   │             │
└─────────┘   └────┬────┘   └──────────────────┘   └──────┬──────┘
                   │                                        │
                   ▼                                        ▼
              ┌─────────┐                            ┌─────────┐
              │Complete │                            │Complete │
              └─────────┘                            └─────────┘
```

**Visual transitions:**
- **Idle → Working**: LiveProgressBar animates in from bottom
- **Working → Waiting**: LiveProgressBar fades, InputArea highlights with subtle pulse on context hint
- **Waiting → Working**: Input area returns to idle style, progress bar re-appears
- **Working → Complete**: Progress bar fades, summary block appears in feed

---

## Streaming Architecture

The SwiftUI layer receives events from the daemon via the Unix socket. Each event maps to a feed mutation:

```swift
enum AgentEvent {
    case thinkingStart(sessionId: String)
    case thinkingChunk(sessionId: String, text: String)
    case thinkingEnd(sessionId: String)
    case toolCallStart(sessionId: String, toolName: String, params: [String: String])
    case toolCallOutput(sessionId: String, output: ToolOutput)
    case toolCallEnd(sessionId: String, status: ToolCallStatus)
    case codeBlock(sessionId: String, content: CodeContent)
    case diffBlock(sessionId: String, content: DiffContent)
    case subagentSpawned(sessionId: String, subagent: SubagentContent)
    case subagentEvent(sessionId: String, subagentId: String, event: AgentEvent)
    case textOutput(sessionId: String, text: String)
    case waitingForInput(sessionId: String, context: String?)
    case waitingForApproval(sessionId: String, action: PendingAction)
    case sessionComplete(sessionId: String)
    case error(sessionId: String, error: ErrorContent)
}
```

**Processing on @Observable model:**
- Events arrive on background queue via daemon socket
- Parsed into `AgentEvent` enum
- Applied to session model on `@MainActor`
- SwiftUI reactivity handles view updates
- Throttled at ~120fps (8ms) to prevent overwhelming the view layer

---

## Performance Guidelines

1. **LazyVStack** — blocks are lazily rendered; only visible blocks are in memory
2. **Stable IDs** — each block has a deterministic ID (derived from event sequence), enabling efficient diffing
3. **Background parsing** — syntax highlighting and diff computation happen off main thread
4. **Throttled updates** — streaming text (thinking) is batched into 8ms frames
5. **View complexity budget** — collapsed blocks are < 5 views deep; expanded blocks cap at ~50 lines visible

---

## Accessibility

- All interactive elements have `.help()` tooltips
- Keyboard navigation: Tab between blocks, Enter to expand/collapse
- VoiceOver: each block announces type + summary (e.g., "Tool call: read file, completed")
- Reduce Motion: disable PulseIndicator animation, use static indicator instead
- High Contrast: border colors increase opacity by 2x

---

## Future Considerations

1. **Side-by-side terminal** — Option to show raw terminal output alongside the hybrid view
2. **Block pinning** — Pin important blocks (diffs, errors) to a sidebar
3. **Search within session** — Cmd+F to search across all blocks
4. **Export** — Export session as markdown/JSON for sharing
5. **Collaboration** — Share live session view with team members
6. **Replay** — Replay completed sessions at configurable speed

---

## Why This Is More Powerful Than a Terminal

| Capability | Terminal | Hybrid Activity View |
|-----------|---------|---------------------|
| See what's happening | Read scrolling text | Glance at progress bar |
| Understand thinking | Parse verbose output | Collapse/expand on demand |
| Review file changes | Read unified diff text | Native diff with colors + stats |
| Track tool calls | Grep for patterns | Visual cards with status lifecycle |
| See subagent work | ? | Nested tree with independent expand |
| Know when input needed | Watch for prompt | Visual mode transition + highlight |
| Copy code | Select text carefully | One-click copy button |
| Navigate history | Scroll and read | Collapse boring parts, expand interesting |
| Understand errors | Red text? Maybe? | Dedicated error blocks with context |
