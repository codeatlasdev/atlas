import Foundation
import os

/// Manages the lifecycle of the atlas-daemon process.
/// Spawns on app launch, terminates on app quit.
@Observable
final class DaemonManager {
    private var process: Process?
    private(set) var isRunning = false
    private let logger = Logger(subsystem: "dev.codeatlas.atlas", category: "daemon")

    /// Path to the daemon binary inside the app bundle
    private var daemonPath: String? {
        Bundle.main.path(forAuxiliaryExecutable: "atlas-daemon")
    }

    /// Start the daemon process
    func start() {
        if isDaemonSocketAvailable() {
            logger.info("Daemon already running (socket available)")
            isRunning = true
            return
        }

        guard let path = daemonPath else {
            startFromCargo()
            return
        }

        spawnDaemon(at: path)
    }

    /// Stop the daemon process
    func stop() {
        guard let process, process.isRunning else { return }
        process.terminate()
        process.waitUntilExit()
        self.process = nil
        isRunning = false
        logger.info("Daemon stopped")
    }

    // MARK: - Private

    private func spawnDaemon(at path: String) {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: path)
        proc.environment = ProcessInfo.processInfo.environment
        proc.standardOutput = FileHandle.nullDevice
        proc.standardError = FileHandle.nullDevice

        proc.terminationHandler = { [weak self] process in
            DispatchQueue.main.async {
                self?.isRunning = false
                self?.logger.warning("Daemon exited with code \(process.terminationStatus)")
                // Auto-restart on unexpected exit
                if process.terminationStatus != 0 {
                    self?.logger.info("Restarting daemon...")
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
            logger.info("Daemon started (pid: \(proc.processIdentifier))")
        } catch {
            logger.error("Failed to start daemon: \(error.localizedDescription)")
        }
    }

    /// Fallback for development: find daemon in cargo target dir
    private func startFromCargo() {
        let possiblePaths = [
            URL(fileURLWithPath: #file)
                .deletingLastPathComponent() // Services/
                .deletingLastPathComponent() // Atlas/
                .deletingLastPathComponent() // Sources/
                .deletingLastPathComponent() // app/
                .appendingPathComponent("target/debug/atlas-daemon").path,
            FileManager.default.currentDirectoryPath + "/../target/debug/atlas-daemon",
            NSHomeDirectory() + "/Projetos/codeatlasdev/atlas/target/debug/atlas-daemon",
        ]

        for path in possiblePaths {
            if FileManager.default.isExecutableFile(atPath: path) {
                logger.info("Found daemon at: \(path)")
                spawnDaemon(at: path)
                return
            }
        }

        logger.warning("Daemon binary not found. Run 'cargo build --bin atlas-daemon' first.")
    }

    /// Check if the daemon socket exists
    private func isDaemonSocketAvailable() -> Bool {
        let socketPath = ("~/.atlas/atlas.sock" as NSString).expandingTildeInPath
        return FileManager.default.fileExists(atPath: socketPath)
    }
}
