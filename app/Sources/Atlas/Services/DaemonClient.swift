import Foundation

@Observable
final class DaemonClient: @unchecked Sendable {
    private(set) var isConnected = false
    private var fileDescriptor: Int32 = -1
    private var pendingRequests: [String: CheckedContinuation<Any, Error>] = [:]
    private let socketPath: String
    private var requestCounter: Int = 0
    private var readTask: Task<Void, Never>?

    /// Notification handlers keyed by method name
    private var notificationHandlers: [String: (NotificationPayload) -> Void] = [:]
    private let lock = NSLock()

    init(socketPath: String = "~/.atlas/atlas.sock") {
        self.socketPath = (socketPath as NSString).expandingTildeInPath
    }

    // MARK: - Connection

    func connect() async throws {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else {
            throw DaemonError.connectionFailed("Failed to create socket: \(String(cString: strerror(errno)))")
        }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)

        let pathBytes = socketPath.utf8CString
        guard pathBytes.count <= MemoryLayout.size(ofValue: addr.sun_path) else {
            close(fd)
            throw DaemonError.connectionFailed("Socket path too long")
        }

        withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
            ptr.withMemoryRebound(to: CChar.self, capacity: pathBytes.count) { dest in
                for i in 0..<pathBytes.count {
                    dest[i] = pathBytes[i]
                }
            }
        }

        let addrLen = socklen_t(MemoryLayout<sockaddr_un>.size)
        let result = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                Darwin.connect(fd, sockPtr, addrLen)
            }
        }

        guard result == 0 else {
            close(fd)
            throw DaemonError.connectionFailed("Connect failed: \(String(cString: strerror(errno)))")
        }

        fileDescriptor = fd
        isConnected = true
        startReadLoop()
    }

    func disconnect() {
        readTask?.cancel()
        readTask = nil
        if fileDescriptor >= 0 {
            close(fileDescriptor)
            fileDescriptor = -1
        }
        isConnected = false

        lock.lock()
        for (_, continuation) in pendingRequests {
            continuation.resume(throwing: DaemonError.disconnected)
        }
        pendingRequests.removeAll()
        notificationHandlers.removeAll()
        lock.unlock()
    }

    // MARK: - RPC

    @discardableResult
    func send(method: String, params: [String: Any] = [:]) async throws -> Any {
        guard fileDescriptor >= 0, isConnected else {
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
        frame.append(0x0A) // newline

        let fd = fileDescriptor
        let writeResult = frame.withUnsafeBytes { ptr in
            Darwin.write(fd, ptr.baseAddress!, ptr.count)
        }

        guard writeResult == frame.count else {
            throw DaemonError.sendFailed("Write failed: \(String(cString: strerror(errno)))")
        }

        return try await withCheckedThrowingContinuation { continuation in
            lock.lock()
            pendingRequests[id] = continuation
            lock.unlock()
        }
    }

    // MARK: - Notifications

    func onNotification(_ method: String, handler: @escaping (NotificationPayload) -> Void) {
        lock.lock()
        notificationHandlers[method] = handler
        lock.unlock()
    }

    func removeNotificationHandler(_ method: String) {
        lock.lock()
        notificationHandlers.removeValue(forKey: method)
        lock.unlock()
    }

    // MARK: - Read Loop

    private func startReadLoop() {
        let fd = fileDescriptor

        readTask = Task.detached { [weak self] in
            var buffer = Data()
            let readBuf = UnsafeMutablePointer<UInt8>.allocate(capacity: 65536)
            defer { readBuf.deallocate() }

            while !Task.isCancelled {
                let bytesRead = Darwin.read(fd, readBuf, 65536)

                if bytesRead <= 0 {
                    await MainActor.run {
                        self?.isConnected = false
                    }
                    break
                }

                buffer.append(readBuf, count: bytesRead)

                // Process complete lines
                while let newlineIndex = buffer.firstIndex(of: 0x0A) {
                    let lineData = buffer[buffer.startIndex..<newlineIndex]
                    buffer.removeSubrange(buffer.startIndex...newlineIndex)

                    guard let self,
                          let json = try? JSONSerialization.jsonObject(with: Data(lineData)) as? [String: Any] else {
                        continue
                    }

                    if let id = json["id"] as? String {
                        self.lock.lock()
                        let continuation = self.pendingRequests.removeValue(forKey: id)
                        self.lock.unlock()

                        if let continuation {
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
                        let params = json["params"] as? [String: Any] ?? [:]
                        let payload = NotificationPayload(method: method, params: params)

                        self.lock.lock()
                        let handler = self.notificationHandlers[method]
                        self.lock.unlock()

                        handler?(payload)
                    }
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
