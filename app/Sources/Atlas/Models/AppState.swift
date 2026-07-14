import Foundation

@Observable
final class AppState {
    var servers: [Server] = []
    var selectedServer: Server?
    var sessions: [Session] = []
    var agentSessions: [AgentSessionInfo] = []
    var isConnected: Bool = false
    var messages: [ChatMessage] = []

    let daemon: DaemonClient

    init(daemon: DaemonClient = DaemonClient()) {
        self.daemon = daemon
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
            // Retry after delay if daemon not available yet
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
        } catch {
            // ignore
        }
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
        } catch {
            // handle error
        }
        return nil
    }

    func stopAgent(sessionId: String) async {
        do {
            try await daemon.send(method: "agent.stop", params: ["session_id": sessionId])
            await refreshAgentSessions()
        } catch {
            // ignore
        }
    }

    func sendPromptToAgent(sessionId: String, prompt: String) async {
        do {
            try await daemon.send(method: "agent.prompt", params: [
                "session_id": sessionId,
                "prompt": prompt
            ])
        } catch {
            // ignore
        }
    }

    // MARK: - Chat

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
