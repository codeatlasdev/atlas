import Foundation
import os

/// Manages the lifecycle of the atlas-daemon process.
/// Spawns on app launch, terminates on app quit.
@Observable
final class DaemonManager {
    private var process: Process?
    private(set) var isRunning = false
    private let logger = Logger(subsystem: "dev.codeatlas.atlas", category: "daemon")

    private let socketPath = ("~/.atlas/atlas.sock" as NSString).expandingTildeInPath

    /// Start the daemon process
    func start() {
        // Remove stale socket if no daemon is actually listening
        cleanStaleSocket()

        // Check if daemon is ACTUALLY responding (not just socket file exists)
        if isDaemonAlive() {
            logger.info("Daemon already running and responding")
            isRunning = true
            return
        }

        // Find daemon binary and spawn
        if let path = findDaemonBinary() {
            spawnDaemon(at: path)
        } else {
            logger.error("Daemon binary not found anywhere. Run 'cargo build --bin atlas-daemon'")
        }
    }

    /// Stop the daemon process
    func stop() {
        if let process, process.isRunning {
            process.terminate()
            process.waitUntilExit()
        }
        self.process = nil
        isRunning = false
        // Clean socket file
        try? FileManager.default.removeItem(atPath: socketPath)
        logger.info("Daemon stopped")
    }

    // MARK: - Private

    private func spawnDaemon(at path: String) {
        // Remove stale socket before starting
        try? FileManager.default.removeItem(atPath: socketPath)

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: path)
        // Pass through PATH so daemon can find kiro-cli etc
        var env = ProcessInfo.processInfo.environment
        env["RUST_LOG"] = env["RUST_LOG"] ?? "atlas=info"
        proc.environment = env
        proc.standardOutput = FileHandle.nullDevice
        proc.standardError = FileHandle.nullDevice

        proc.terminationHandler = { [weak self] process in
            DispatchQueue.main.async {
                self?.isRunning = false
                if process.terminationStatus != 0 && process.terminationStatus != 15 {
                    self?.logger.warning("Daemon crashed (code \(process.terminationStatus)), restarting...")
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                        self?.spawnDaemon(at: path)
                    }
                }
            }
        }

        do {
            try proc.run()
            self.process = proc
            isRunning = true
            logger.info("Daemon started (pid: \(proc.processIdentifier), path: \(path))")
        } catch {
            logger.error("Failed to start daemon: \(error.localizedDescription)")
        }
    }

    /// Try to connect to socket to verify daemon is actually alive
    private func isDaemonAlive() -> Bool {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return false }
        defer { close(fd) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = socketPath.utf8CString
        withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
            ptr.withMemoryRebound(to: CChar.self, capacity: pathBytes.count) { dest in
                for i in 0..<pathBytes.count { dest[i] = pathBytes[i] }
            }
        }

        let result = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                Darwin.connect(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }

        return result == 0
    }

    /// Remove socket file if daemon isn't actually running
    private func cleanStaleSocket() {
        guard FileManager.default.fileExists(atPath: socketPath) else { return }
        if !isDaemonAlive() {
            try? FileManager.default.removeItem(atPath: socketPath)
            logger.info("Removed stale socket file")
        }
    }

    /// Find daemon binary: bundle first, then cargo target
    private func findDaemonBinary() -> String? {
        // 1. In app bundle (production)
        if let bundled = Bundle.main.path(forAuxiliaryExecutable: "atlas-daemon") {
            return bundled
        }

        // 2. Cargo target directory (development)
        let devPaths = [
            // Relative to the workspace root
            NSHomeDirectory() + "/Projetos/codeatlasdev/atlas/target/debug/atlas-daemon",
            // Try current working directory patterns
            FileManager.default.currentDirectoryPath + "/target/debug/atlas-daemon",
            // Global cargo bin
            NSHomeDirectory() + "/.cargo/bin/atlas-daemon",
        ]

        for path in devPaths {
            if FileManager.default.isExecutableFile(atPath: path) {
                return path
            }
        }

        return nil
    }
}
