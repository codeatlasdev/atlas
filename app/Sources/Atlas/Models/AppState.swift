import Foundation

@Observable
final class AppState {
    var servers: [Server] = []
    var selectedServer: Server?
    var sessions: [Session] = []
    var isConnected: Bool = false
    var messages: [ChatMessage] = []

    let daemon: DaemonClient

    init(daemon: DaemonClient = DaemonClient()) {
        self.daemon = daemon
    }

    func connect() async {
        do {
            try await daemon.connect()
            isConnected = true
            await refreshServers()
            await refreshSessions()
        } catch {
            isConnected = false
        }
    }

    func disconnect() {
        daemon.disconnect()
        isConnected = false
    }

    func refreshServers() async {
        do {
            let response = try await daemon.send(method: "servers.list")
            if let data = try? JSONSerialization.data(withJSONObject: response),
               let decoded = try? JSONDecoder.atlas.decode([Server].self, from: data) {
                servers = decoded
            }
        } catch {
            // Connection may have dropped
            isConnected = false
        }
    }

    func refreshSessions() async {
        do {
            let response = try await daemon.send(method: "sessions.list")
            if let data = try? JSONSerialization.data(withJSONObject: response),
               let decoded = try? JSONDecoder.atlas.decode([Session].self, from: data) {
                sessions = decoded
            }
        } catch {
            isConnected = false
        }
    }

    func sendChat(message: String) async {
        let userMessage = ChatMessage(role: .user, content: message)
        messages.append(userMessage)

        do {
            let params: [String: Any] = ["message": message]
            let response = try await daemon.send(method: "ai.chat", params: params)
            if let content = response as? [String: Any],
               let text = content["content"] as? String {
                let assistantMessage = ChatMessage(role: .assistant, content: text)
                messages.append(assistantMessage)
            }
        } catch {
            let errorMessage = ChatMessage(role: .system, content: "Error: \(error.localizedDescription)")
            messages.append(errorMessage)
        }
    }
}

struct ChatMessage: Identifiable, Hashable {
    let id = UUID()
    let role: ChatRole
    let content: String
    let timestamp = Date()
}

enum ChatRole: String, Hashable {
    case user
    case assistant
    case system
}

extension JSONDecoder {
    static let atlas: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
