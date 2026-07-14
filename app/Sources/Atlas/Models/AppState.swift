import Foundation
import AppKit

@Observable
final class AppState {
    // MARK: - Project State

    var currentProject: ProjectInfo?
    var recentProjects: [ProjectInfo] = []
    var selectedTab: SidebarTab = .sessions
    var needsProjectSetup: Bool = false
    var projectDetection: ProjectDetectionResult?

    // MARK: - Kanban

    var tasks: [TaskItem] = []

    // MARK: - Tech Lead Chat

    var techLeadMessages: [ChatMessage] = []
    var isTechLeadTyping: Bool = false
    var techLeadCurrentActivity: String = ""
    var isTechLeadStreaming: Bool = false
    var techLeadSessionId: String?
    var techLeadTerminalId: String?
    private var isSubscribedToAgentEvents = false

    // MARK: - Existing State

    var servers: [Server] = []
    var selectedServer: Server?
    var sessions: [Session] = []
    var agentSessions: [AgentSessionInfo] = []
    var isConnected: Bool = false
    var messages: [ChatMessage] = []

    let daemon: DaemonClient

    init(daemon: DaemonClient = DaemonClient()) {
        self.daemon = daemon
        loadRecentProjects()
    }

    // MARK: - Project Management

    func openProject(path: String) {
        let name = (path as NSString).lastPathComponent
        let project = ProjectInfo(name: name, path: path, lastOpened: .now)
        currentProject = project
        addToRecentProjects(project)
        selectedTab = .sessions

        // Check if atlas.yaml exists — show setup wizard if not
        let yamlPath = (path as NSString).appendingPathComponent("atlas.yaml")
        if FileManager.default.fileExists(atPath: yamlPath) {
            needsProjectSetup = false
        } else {
            needsProjectSetup = true
        }

        if let sound = NSSound(named: .init("Morse")), !sound.isPlaying {
            sound.play()
        }

        Task {
            await refreshTasks()
            // Re-attach to Tech Lead terminal if session already exists
            if techLeadSessionId != nil, techLeadTerminalId != nil {
                subscribeToTechLeadOutput()
            }
        }
    }

    func closeProject() {
        currentProject = nil
        tasks = []
        techLeadMessages = []
    }

    func loadRecentProjects() {
        guard let data = UserDefaults.standard.data(forKey: "atlas.recentProjects"),
              let decoded = try? JSONDecoder().decode([ProjectInfo].self, from: data) else {
            return
        }
        recentProjects = decoded
    }

    private func addToRecentProjects(_ project: ProjectInfo) {
        recentProjects.removeAll { $0.path == project.path }
        recentProjects.insert(project, at: 0)
        if recentProjects.count > 10 {
            recentProjects = Array(recentProjects.prefix(10))
        }
        saveRecentProjects()
    }

    private func saveRecentProjects() {
        if let data = try? JSONEncoder().encode(recentProjects) {
            UserDefaults.standard.set(data, forKey: "atlas.recentProjects")
        }
    }

    func openProjectPicker() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.message = "Select a project folder"
        panel.prompt = "Open"

        if panel.runModal() == .OK, let url = panel.url {
            openProject(path: url.path)
        }
    }

    // MARK: - Kanban

    func refreshTasks() async {
        guard let project = currentProject else { return }
        do {
            let result = try await daemon.send(method: "tasks.list", params: [
                "project_path": project.path
            ])
            if let array = result as? [[String: Any]] {
                tasks = array.compactMap { dict -> TaskItem? in
                    guard let id = dict["id"] as? String,
                          let title = dict["title"] as? String else { return nil }
                    return TaskItem(
                        id: UUID(uuidString: id) ?? UUID(),
                        title: title,
                        description: dict["description"] as? String ?? "",
                        status: TaskStatus.from(dict["status"] as? String ?? "todo"),
                        priority: TaskPriority.from(dict["priority"] as? String ?? "medium"),
                        assignedAgent: dict["assigned_agent"] as? String,
                        labels: (dict["labels"] as? [String]) ?? []
                    )
                }
            }
        } catch {
            // If daemon unavailable, keep existing tasks
        }
    }

    func moveTask(_ task: TaskItem, to status: TaskStatus) {
        guard let index = tasks.firstIndex(where: { $0.id == task.id }) else { return }
        tasks[index].status = status

        Task {
            try? await daemon.send(method: "tasks.update_status", params: [
                "id": task.id.uuidString,
                "status": status.rawValue
            ])
        }
    }

    func createTask(title: String, description: String, priority: TaskPriority) {
        let task = TaskItem(
            id: UUID(),
            title: title,
            description: description,
            status: .backlog,
            priority: priority,
            assignedAgent: nil,
            labels: []
        )
        tasks.append(task)

        guard let project = currentProject else { return }
        Task {
            try? await daemon.send(method: "tasks.create", params: [
                "project_path": project.path,
                "title": title,
                "description": description,
                "priority": priority.rawValue
            ])
        }
    }

    // MARK: - Tech Lead

    func sendToTechLead(message: String) async {
        guard let project = currentProject else { return }

        let userMsg = ChatMessage(role: .user, content: message)
        techLeadMessages.append(userMsg)
        isTechLeadTyping = true

        do {
            let response = try await daemon.send(method: "techlead.chat", params: [
                "message": message,
                "project_path": project.path
            ])

            if let dict = response as? [String: Any] {
                let action = dict["action"] as? String ?? ""
                let sessionId = dict["session_id"] as? String
                let protocol_ = dict["protocol"] as? String ?? "pty"

                if let sid = sessionId {
                    techLeadSessionId = sid

                    if protocol_ == "acp" && !isSubscribedToAgentEvents {
                        await subscribeToAgentEvents(sessionId: sid)
                    } else if protocol_ != "acp", let tid = dict["terminal_session_id"] as? String {
                        techLeadTerminalId = tid
                        subscribeToTechLeadOutput()
                    }
                }

                if action == "spawned" {
                    techLeadMessages.append(ChatMessage(
                        role: .system,
                        content: "⚡ Tech Lead session started."
                    ))

                    // For new sessions: send the initial prompt (with steering) AFTER subscribing
                    if let sid = sessionId, protocol_ == "acp" {
                        let prompt = dict["initial_prompt"] as? String ?? message
                        let _ = try? await daemon.send(method: "agent.prompt", params: [
                            "session_id": sid,
                            "prompt": prompt
                        ])
                    }
                }
            }
        } catch {
            techLeadMessages.append(ChatMessage(
                role: .system,
                content: "Error: \(error.localizedDescription)"
            ))
            isTechLeadTyping = false
        }
    }

    /// Subscribe to structured ACP agent events for the Tech Lead session
    func subscribeToAgentEvents(sessionId: String) async {
        // Register notification handler FIRST
        isSubscribedToAgentEvents = true

        daemon.onNotification("agent.event", id: "techlead-acp") { [weak self] payload in
            guard let self else { return }

            guard let eventData = try? JSONSerialization.data(withJSONObject: payload.params) else {
                #if DEBUG
                print("[Atlas] agent.event: can't serialize params")
                #endif
                return
            }

            do {
                let event = try JSONDecoder().decode(AgentEvent.self, from: eventData)
                DispatchQueue.main.async {
                    self.handleAgentEvent(event)
                }
            } catch {
                #if DEBUG
                print("[Atlas] agent.event decode error: \(error)")
                if let str = String(data: eventData, encoding: .utf8) {
                    print("[Atlas] raw: \(str.prefix(200))")
                }
                #endif
            }
        }

        // Subscribe RPC — await to ensure daemon starts broadcasting before we return
        let _ = try? await daemon.send(method: "agent.subscribe", params: [
            "session_id": sessionId
        ])
    }

    @MainActor
    private func handleAgentEvent(_ event: AgentEvent) {
        switch event.event {
        case .textChunk(let chunk):
            isTechLeadTyping = false
            isTechLeadStreaming = true
            techLeadCurrentActivity = ""
            // Append to last assistant message or create new
            if let lastIndex = techLeadMessages.indices.last,
               techLeadMessages[lastIndex].role == .assistant {
                let existing = techLeadMessages[lastIndex].content
                techLeadMessages[lastIndex] = ChatMessage(
                    role: .assistant,
                    content: existing + chunk.text
                )
            } else {
                techLeadMessages.append(ChatMessage(
                    role: .assistant,
                    content: chunk.text
                ))
            }

        case .thinkingChunk:
            isTechLeadTyping = true
            isTechLeadStreaming = false
            techLeadCurrentActivity = "Pensando..."

        case .toolCallStart(let tc):
            isTechLeadTyping = true
            isTechLeadStreaming = false
            techLeadCurrentActivity = tc.title.isEmpty ? tc.toolName : tc.title

        case .toolCallUpdate(let update):
            if update.status == "completed" || update.status == "failed" {
                techLeadCurrentActivity = ""
            }

        case .turnEnd:
            isTechLeadTyping = false
            isTechLeadStreaming = false
            techLeadCurrentActivity = ""

        default:
            break
        }
    }

    /// Accumulated buffer for streaming output (debounced)
    private var outputBuffer = ""
    private var outputFlushTask: Task<Void, Never>?

    func subscribeToTechLeadOutput() {
        guard let terminalId = techLeadTerminalId else { return }

        Task {
            let _ = try? await daemon.send(method: "terminal.attach", params: [
                "session_id": terminalId
            ])
        }

        daemon.onNotification("terminal.output", id: "techlead-output") { [weak self] payload in
            guard let self,
                  let sid = payload.string(forKey: "session_id"),
                  sid == self.techLeadTerminalId,
                  let data = payload.data(forKey: "data"),
                  let text = String(data: data, encoding: .utf8) else { return }

            // Strip ANSI escape sequences (comprehensive)
            let clean = self.stripAnsi(text)
            guard !clean.isEmpty else { return }

            DispatchQueue.main.async {
                self.outputBuffer += clean
                self.isTechLeadTyping = false
                self.scheduleFlush()
            }
        }
    }

    /// Debounce: flush accumulated output every 300ms into a chat message
    private func scheduleFlush() {
        outputFlushTask?.cancel()
        outputFlushTask = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(300))
            guard !Task.isCancelled else { return }
            flushOutputBuffer()
        }
    }

    @MainActor
    private func flushOutputBuffer() {
        let text = outputBuffer.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        outputBuffer = ""

        // If last message is assistant, append to it (streaming continuation)
        if let lastIndex = techLeadMessages.indices.last,
           techLeadMessages[lastIndex].role == .assistant {
            let existing = techLeadMessages[lastIndex].content
            techLeadMessages[lastIndex] = ChatMessage(
                role: .assistant,
                content: existing + text
            )
        } else {
            // New assistant message
            techLeadMessages.append(ChatMessage(
                role: .assistant,
                content: text
            ))
        }
    }

    /// Strip all ANSI escape sequences from terminal output
    private func stripAnsi(_ text: String) -> String {
        // Covers: CSI sequences, OSC sequences, simple escapes
        text.replacingOccurrences(
            of: "\\x1B(?:\\[[0-9;?]*[A-Za-z]|\\].*?(?:\\x07|\\x1B\\\\)|[()][0-9A-Za-z]|[>=<])",
            with: "",
            options: .regularExpression
        )
        .replacingOccurrences(of: "\r", with: "")
    }

    // MARK: - Connection

    func connect() async {
        // Wait for daemon socket to appear (max 5 seconds)
        let socketPath = ("~/.atlas/atlas.sock" as NSString).expandingTildeInPath
        for _ in 0..<50 {
            if FileManager.default.fileExists(atPath: socketPath) {
                break
            }
            try? await Task.sleep(for: .milliseconds(100))
        }

        do {
            try await daemon.connect()
            isConnected = true
            await refreshServers()
            await refreshAgentSessions()
        } catch {
            isConnected = false
            try? await Task.sleep(for: .seconds(3))
            await connect()
        }
    }

    func disconnect() {
        daemon.disconnect()
        isConnected = false
    }

    // MARK: - Servers

    func refreshServers() async {
        do {
            let response = try await daemon.send(method: "servers.list")
            if let data = try? JSONSerialization.data(withJSONObject: response),
               let decoded = try? JSONDecoder.atlas.decode([Server].self, from: data) {
                servers = decoded
            }
        } catch {
            isConnected = false
        }
    }

    // MARK: - Agent Sessions

    func refreshAgentSessions() async {
        do {
            let response = try await daemon.send(method: "agent.list")
            if let array = response as? [[String: Any]] {
                agentSessions = array.compactMap { dict in
                    guard let id = dict["id"] as? String,
                          let adapter = dict["adapter"] as? String,
                          let state = dict["activity_state"] as? String else { return nil }
                    return AgentSessionInfo(
                        id: id,
                        adapter: adapter,
                        terminalSessionId: dict["terminal_session_id"] as? String,
                        protocol: dict["protocol"] as? String,
                        activityState: state,
                        startedAt: dict["started_at"] as? String
                    )
                }
            }
        } catch {}
    }

    func spawnAgent(adapter: String, prompt: String, cwd: String = "~") async -> String? {
        do {
            let result = try await daemon.send(method: "agent.spawn", params: [
                "adapter": adapter,
                "prompt": prompt,
                "cwd": (cwd as NSString).expandingTildeInPath,
                "permission": "autonomous"
            ])
            if let dict = result as? [String: Any],
               let sessionId = dict["session_id"] as? String {
                await refreshAgentSessions()
                return sessionId
            }
        } catch {}
        return nil
    }

    func stopAgent(sessionId: String) async {
        do {
            try await daemon.send(method: "agent.stop", params: ["session_id": sessionId])
            await refreshAgentSessions()
        } catch {}
    }

    func sendPromptToAgent(sessionId: String, prompt: String) async {
        do {
            try await daemon.send(method: "agent.prompt", params: [
                "session_id": sessionId,
                "prompt": prompt
            ])
        } catch {}
    }

    // MARK: - Chat (legacy)

    func sendChat(message: String) async {
        let userMessage = ChatMessage(role: .user, content: message)
        messages.append(userMessage)

        do {
            let response = try await daemon.send(method: "ai.chat", params: ["message": message])
            if let content = response as? [String: Any],
               let text = content["content"] as? String {
                messages.append(ChatMessage(role: .assistant, content: text))
            }
        } catch {
            messages.append(ChatMessage(role: .system, content: "Error: \(error.localizedDescription)"))
        }
    }
}

// MARK: - Supporting Types

enum SidebarTab: String, CaseIterable, Hashable {
    case sessions = "Sessions"
    case techLead = "Tech Lead"
    case servers = "Servers"
    case deploy = "Deploy"

    var icon: String {
        switch self {
        case .sessions: "square.grid.3x3.topleft.filled"
        case .techLead: "brain.head.profile"
        case .servers: "server.rack"
        case .deploy: "paperplane.fill"
        }
    }
}

struct ProjectInfo: Codable, Identifiable, Hashable {
    var id: String { path }
    let name: String
    let path: String
    let lastOpened: Date
}

struct TaskItem: Identifiable, Hashable {
    let id: UUID
    var title: String
    var description: String
    var status: TaskStatus
    var priority: TaskPriority
    var assignedAgent: String?
    var labels: [String]

    static let samples: [TaskItem] = [
        TaskItem(id: UUID(), title: "Setup CI pipeline", description: "Configure GitHub Actions", status: .todo, priority: .high, assignedAgent: "kiro", labels: ["infra"]),
        TaskItem(id: UUID(), title: "Add auth module", description: "OAuth2 + JWT", status: .inProgress, priority: .critical, assignedAgent: "claude-code", labels: ["auth", "backend"]),
        TaskItem(id: UUID(), title: "Design system tokens", description: "Extract colors and typography", status: .done, priority: .medium, assignedAgent: nil, labels: ["design"]),
        TaskItem(id: UUID(), title: "API rate limiting", description: "Implement rate limiter middleware", status: .review, priority: .high, assignedAgent: "kiro", labels: ["backend"]),
        TaskItem(id: UUID(), title: "Database migrations", description: "Setup sqlx migrations", status: .backlog, priority: .low, assignedAgent: nil, labels: ["infra", "db"]),
    ]
}

enum TaskStatus: String, CaseIterable, Hashable {
    case backlog = "Backlog"
    case todo = "Todo"
    case inProgress = "In Progress"
    case review = "Review"
    case done = "Done"

    static func from(_ string: String) -> Self {
        switch string.lowercased() {
        case "backlog": .backlog
        case "todo": .todo
        case "in_progress", "inprogress", "in progress": .inProgress
        case "review": .review
        case "done": .done
        default: .todo
        }
    }

    var color: Color {
        switch self {
        case .backlog: AtlasColors.textTertiary
        case .todo: AtlasColors.neonAmber
        case .inProgress: AtlasColors.neonCyan
        case .review: AtlasColors.neonPurple
        case .done: AtlasColors.neonGreen
        }
    }
}

enum TaskPriority: String, CaseIterable, Hashable {
    case low = "Low"
    case medium = "Medium"
    case high = "High"
    case critical = "Critical"

    static func from(_ string: String) -> Self {
        switch string.lowercased() {
        case "low": .low
        case "medium": .medium
        case "high": .high
        case "critical": .critical
        default: .medium
        }
    }

    var color: Color {
        switch self {
        case .low: AtlasColors.textTertiary
        case .medium: AtlasColors.neonAmber
        case .high: AtlasColors.neonPink
        case .critical: AtlasColors.neonRed
        }
    }
}

import SwiftUI

struct AgentSessionInfo: Identifiable, Hashable {
    let id: String
    let adapter: String
    let terminalSessionId: String?
    let `protocol`: String?
    let activityState: String
    let startedAt: String?
}

struct ChatMessage: Identifiable, Hashable {
    let id = UUID()
    let role: ChatRole
    let content: String
    let timestamp = Date()
}

enum ChatRole: String, Hashable {
    case user, assistant, system
}

extension JSONDecoder {
    static let atlas: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
