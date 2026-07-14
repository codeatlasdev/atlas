import Foundation
import Network

@Observable
final class DaemonClient {
    private(set) var connectionState: NWConnection.State = .setup
    private var connection: NWConnection?
    private var pendingRequests: [String: CheckedContinuation<Any, Error>] = [:]
    private let socketPath: String
    private var requestCounter: Int = 0

    var isConnected: Bool {
        connectionState == .ready
    }

    init(socketPath: String = "~/.atlas/atlas.sock") {
        self.socketPath = (socketPath as NSString).expandingTildeInPath
    }

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
        // Fail any pending requests
        for (_, continuation) in pendingRequests {
            continuation.resume(throwing: DaemonError.disconnected)
        }
        pendingRequests.removeAll()
    }

    @discardableResult
    func send(method: String, params: [String: Any] = [:]) async throws -> Any {
        guard let connection, connectionState == .ready else {
            throw DaemonError.notConnected
        }

        requestCounter += 1
        let id = "req-\(requestCounter)"

        var payload: [String: Any] = [
            "method": method,
            "id": id,
        ]
        if !params.isEmpty {
            payload["params"] = params
        }

        let data = try JSONSerialization.data(withJSONObject: payload)
        // Newline-delimited JSON protocol
        var frame = data
        frame.append(0x0A) // '\n'

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

    private func startReceiving() {
        receiveLoop()
    }

    private func receiveLoop() {
        guard let connection else { return }

        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] content, _, isComplete, error in
            guard let self else { return }

            if let data = content {
                self.handleReceived(data: data)
            }

            if isComplete || error != nil {
                self.connectionState = .cancelled
                return
            }

            self.receiveLoop()
        }
    }

    private func handleReceived(data: Data) {
        // Split by newlines for ndjson
        let lines = data.split(separator: 0x0A)
        for line in lines {
            guard let json = try? JSONSerialization.jsonObject(with: Data(line)) as? [String: Any] else {
                continue
            }
            guard let id = json["id"] as? String else { continue }

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
        }
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
