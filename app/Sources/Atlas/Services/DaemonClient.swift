import Foundation
import Network

@Observable
final class DaemonClient {
    private(set) var connectionState: NWConnection.State = .setup
    private var connection: NWConnection?
    private var pendingRequests: [String: CheckedContinuation<Any, Error>] = [:]
    private let socketPath: String
    private var requestCounter: Int = 0
    private var buffer = Data()

    /// Notification handlers keyed by method name
    private var notificationHandlers: [String: (NotificationPayload) -> Void] = [:]

    var isConnected: Bool {
        connectionState == .ready
    }

    init(socketPath: String = "~/.atlas/atlas.sock") {
        self.socketPath = (socketPath as NSString).expandingTildeInPath
    }

    // MARK: - Connection

    func connect() async throws {
        let endpoint = NWEndpoint.unix(path: socketPath)
        let parameters = NWParameters()
        parameters.defaultProtocolStack.transportProtocol = NWProtocolTCP.Options()

        let conn = NWConnection(to: endpoint, using: parameters)
        connection = conn

        return try await withCheckedThrowingContinuation { continuation in
            conn.stateUpdateHandler = { [weak self] state in
                self?.connectionState = state
                switch state {
                case .ready:
                    continuation.resume()
                    self?.startReceiving()
                case .failed(let error):
                    continuation.resume(throwing: DaemonError.connectionFailed(error.localizedDescription))
                case .cancelled:
                    break
                default:
                    break
                }
            }
            conn.start(queue: .global(qos: .userInitiated))
        }
    }

    func disconnect() {
        connection?.cancel()
        connection = nil
        connectionState = .cancelled
        for (_, continuation) in pendingRequests {
            continuation.resume(throwing: DaemonError.disconnected)
        }
        pendingRequests.removeAll()
        notificationHandlers.removeAll()
    }

    // MARK: - RPC

    @discardableResult
    func send(method: String, params: [String: Any] = [:]) async throws -> Any {
        guard let connection, connectionState == .ready else {
            throw DaemonError.notConnected
        }

        requestCounter += 1
        let id = "req-\(requestCounter)"

        var payload: [String: Any] = ["method": method, "id": id]
        if !params.isEmpty {
            payload["params"] = params
        }

        let data = try JSONSerialization.data(withJSONObject: payload)
        var frame = data
        frame.append(0x0A)

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            connection.send(content: frame, completion: .contentProcessed { error in
                if let error {
                    continuation.resume(throwing: DaemonError.sendFailed(error.localizedDescription))
                } else {
                    continuation.resume()
                }
            })
        }

        return try await withCheckedThrowingContinuation { continuation in
            pendingRequests[id] = continuation
        }
    }

    // MARK: - Notifications

    /// Register a handler for server-push notifications (e.g. "terminal.output")
    func onNotification(_ method: String, handler: @escaping (NotificationPayload) -> Void) {
        notificationHandlers[method] = handler
    }

    /// Remove notification handler
    func removeNotificationHandler(_ method: String) {
        notificationHandlers.removeValue(forKey: method)
    }

    // MARK: - Receive Loop

    private func startReceiving() {
        receiveLoop()
    }

    private func receiveLoop() {
        guard let connection else { return }

        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] content, _, isComplete, error in
            guard let self else { return }

            if let data = content {
                self.buffer.append(data)
                self.processBuffer()
            }

            if isComplete || error != nil {
                self.connectionState = .cancelled
                return
            }

            self.receiveLoop()
        }
    }

    private func processBuffer() {
        while let newlineIndex = buffer.firstIndex(of: 0x0A) {
            let lineData = buffer[buffer.startIndex..<newlineIndex]
            buffer.removeSubrange(buffer.startIndex...newlineIndex)

            guard let json = try? JSONSerialization.jsonObject(with: Data(lineData)) as? [String: Any] else {
                continue
            }

            if let id = json["id"] as? String {
                // Response to a pending request
                if let continuation = pendingRequests.removeValue(forKey: id) {
                    if let error = json["error"] as? [String: Any],
                       let message = error["message"] as? String {
                        continuation.resume(throwing: DaemonError.rpcError(message))
                    } else if let result = json["result"] {
                        continuation.resume(returning: result)
                    } else {
                        continuation.resume(returning: NSNull())
                    }
                }
            } else if let method = json["method"] as? String {
                // Server-push notification (no id)
                let params = json["params"] as? [String: Any] ?? [:]
                let payload = NotificationPayload(method: method, params: params)

                if let handler = notificationHandlers[method] {
                    handler(payload)
                }
            }
        }
    }
}

// MARK: - Types

struct NotificationPayload {
    let method: String
    let params: [String: Any]

    func string(forKey key: String) -> String? {
        params[key] as? String
    }

    func data(forKey key: String) -> Data? {
        guard let base64 = params[key] as? String else { return nil }
        return Data(base64Encoded: base64)
    }
}

enum DaemonError: LocalizedError {
    case notConnected
    case disconnected
    case connectionFailed(String)
    case sendFailed(String)
    case rpcError(String)
    case invalidResponse

    var errorDescription: String? {
        switch self {
        case .notConnected: "Not connected to daemon"
        case .disconnected: "Connection was closed"
        case .connectionFailed(let reason): "Connection failed: \(reason)"
        case .sendFailed(let reason): "Send failed: \(reason)"
        case .rpcError(let message): "RPC error: \(message)"
        case .invalidResponse: "Invalid response from daemon"
        }
    }
}
