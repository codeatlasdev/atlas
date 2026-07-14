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
        return tv
    }

    func updateNSView(_ tv: TerminalView, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(sessionId: sessionId, client: client)
    }

    class Coordinator: NSObject, TerminalViewDelegate {
        let sessionId: String
        let client: DaemonClient

        init(sessionId: String, client: DaemonClient) {
            self.sessionId = sessionId
            self.client = client
        }

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
