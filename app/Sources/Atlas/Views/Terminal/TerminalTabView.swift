import SwiftUI
import SwiftTerm

struct TerminalTabView: View {
    let sessionId: String
    @Environment(AppState.self) private var appState

    var body: some View {
        TerminalRepresentable(sessionId: sessionId, client: appState.daemon)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct TerminalRepresentable: NSViewRepresentable {
    let sessionId: String
    let client: DaemonClient

    func makeNSView(context: Context) -> TerminalView {
        let tv = TerminalView(frame: .zero)
        tv.terminalDelegate = context.coordinator
        context.coordinator.terminalView = tv
        context.coordinator.attach()
        return tv
    }

    func updateNSView(_ tv: TerminalView, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(sessionId: sessionId, client: client)
    }

    class Coordinator: NSObject, TerminalViewDelegate {
        let sessionId: String
        let client: DaemonClient
        weak var terminalView: TerminalView?

        init(sessionId: String, client: DaemonClient) {
            self.sessionId = sessionId
            self.client = client
        }

        /// Attach to daemon session: get scrollback + subscribe to output
        func attach() {
            Task {
                // 1. Call terminal.attach to get scrollback
                let result = try? await client.send(
                    method: "terminal.attach",
                    params: ["session_id": sessionId]
                )

                if let dict = result as? [String: Any],
                   let scrollbackB64 = dict["scrollback"] as? String,
                   let scrollbackData = Data(base64Encoded: scrollbackB64) {
                    await MainActor.run {
                        let bytes = Array(scrollbackData)
                        terminalView?.feed(byteArray: bytes[...])
                    }
                }

                // 2. Register for terminal.output notifications
                client.onNotification("terminal.output") { [weak self] payload in
                    guard let self,
                          let sid = payload.string(forKey: "session_id"),
                          sid == self.sessionId,
                          let data = payload.data(forKey: "data") else { return }

                    DispatchQueue.main.async {
                        let bytes = Array(data)
                        self.terminalView?.feed(byteArray: bytes[...])
                    }
                }
            }
        }

        // MARK: - TerminalViewDelegate

        func send(source: TerminalView, data: ArraySlice<UInt8>) {
            let encoded = Data(Array(data)).base64EncodedString()
            Task {
                try? await client.send(method: "terminal.input", params: [
                    "session_id": sessionId,
                    "data": encoded,
                ])
            }
        }

        func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {
            Task {
                try? await client.send(method: "terminal.resize", params: [
                    "session_id": sessionId,
                    "rows": newRows,
                    "cols": newCols,
                ])
            }
        }

        func scrolled(source: TerminalView, position: Double) {}
        func setTerminalTitle(source: TerminalView, title: String) {}
        func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}
        func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {}
        func rangeChanged(source: TerminalView, startY: Int, endY: Int) {}
    }
}
