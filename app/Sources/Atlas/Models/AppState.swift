import Foundation
import AppKit

@Observable
final class AppState {
    // MARK: - Project State

    var currentProject: ProjectInfo?
    var recentProjects: [ProjectInfo] = []
    var selectedTab: SidebarTab = .kanban
    var needsProjectSetup: Bool = false
    var projectDetection: ProjectDetectionResult?

    // MARK: - Kanban

    var tasks: [TaskItem] = []

    // MARK: - Tech Lead Chat

    var techLeadMessages: [ChatMessage] = []
    var isTechLeadTyping: Bool = false
    var techLeadSessionId: String?
    var techLeadTerminalId: String?

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
        selectedTab = .kanban

        // Check if atlas.yaml exists — show setup wizard if not
        let yamlPath = (path as NSString).appendingPathComponent("atlas.yaml")
        if FileManager.default.fileExists(atPath: yamlPath) {
            needsProjectSetup = false
        } else {
            needsProjectSetup = true
        }

        NSSound(named: .init("Morse"))?.play()

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

                if action == "spawned", let sid = sessionId {
                    techLeadSessionId = sid
                    await refreshAgentSessions()
                    if let agent = agentSessions.first(where: { $0.id == sid }) {
                        techLeadTerminalId = agent.terminalSessionId
                        subscribeToTechLeadOutput()
                    }
                    techLeadMessages.append(ChatMessage(
                        role: .system,
                        content: "⚡ Tech Lead session started. Kiro is processing..."
                    ))
                } else if action == "message_sent" {
                    // Message sent to existing session, output will come via terminal
                }
            }
        } catch {
            techLeadMessages.append(ChatMessage(
                role: .system,
                content: "Error: \(error.localizedDescription)"
            ))
        }

        isTechLeadTyping = false
    }

    func subscribeToTechLeadOutput() {
        guard let terminalId = techLeadTerminalId else { return }

        Task {
            let _ = try? await daemon.send(method: "terminal.attach", params: [
                "session_id": terminalId
            ])
        }

        daemon.onNotification("terminal.output") { [weak self] payload in
            guard let self,
                  let sid = payload.string(forKey: "session_id"),
                  sid == self.techLeadTerminalId,
                  let data = payload.data(forKey: "data"),
                  let text = String(data: data, encoding: .utf8) else { return }

            let cleanText = text.replacingOccurrences(
                of: "\\x1B\\[[0-9;]*[A-Za-z]",
                with: "",
                options: .regularExpression
            )

            let trimmed = cleanText.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { return }

            DispatchQueue.main.async {
                if let last = self.techLeadMessages.last, last.role == .assistant {
                    self.techLeadMessages[self.techLeadMessages.count - 1] = ChatMessage(
                        role: .assistant,
                        content: last.content + trimmed
                    )
                } else {
                    self.techLeadMessages.append(ChatMessage(
                        role: .assistant,
                        content: trimmed
                    ))
                }
                self.isTechLeadTyping = false
            }
        }
    }

    // MARK: - Connection

    func connect() async {
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
    case kanban = "Kanban"
    case techLead = "Tech Lead"
    case agents = "Agents"
    case servers = "Servers"
    case deploy = "Deploy"

    var icon: String {
        switch self {
        case .kanban: "rectangle.3.group"
        case .techLead: "brain.head.profile"
        case .agents: "cpu"
        case .servers: "server.rack"
        case .deploy: "rocket"
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
