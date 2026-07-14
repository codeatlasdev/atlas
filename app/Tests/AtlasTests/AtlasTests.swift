import Foundation
import Testing
@testable import Atlas

@Suite("Atlas Models")
struct AtlasTests {
    @Test("Server decodes from JSON")
    func serverDecoding() throws {
        let json = """
        {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "production",
            "host": "192.168.1.100",
            "user": "deploy",
            "port": 22,
            "status": "online",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        }
        """
        let data = Data(json.utf8)
        let server = try JSONDecoder.atlas.decode(Server.self, from: data)

        #expect(server.name == "production")
        #expect(server.host == "192.168.1.100")
        #expect(server.user == "deploy")
        #expect(server.port == 22)
        #expect(server.status == .online)
    }

    @Test("SystemdService decodes from JSON")
    func serviceDecoding() throws {
        let json = """
        {
            "id": "550e8400-e29b-41d4-a716-446655440001",
            "server_id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "nginx",
            "unit_name": "nginx.service",
            "state": "running",
            "enabled": true,
            "created_at": "2024-01-01T00:00:00Z"
        }
        """
        let data = Data(json.utf8)
        let service = try JSONDecoder.atlas.decode(SystemdService.self, from: data)

        #expect(service.name == "nginx")
        #expect(service.unitName == "nginx.service")
        #expect(service.state == .running)
        #expect(service.enabled == true)
    }

    @Test("Session decodes from JSON")
    func sessionDecoding() throws {
        let json = """
        {
            "id": "550e8400-e29b-41d4-a716-446655440002",
            "kind": "ssh",
            "server_id": "550e8400-e29b-41d4-a716-446655440000",
            "started_at": "2024-01-01T10:00:00Z",
            "ended_at": null,
            "metadata": {}
        }
        """
        let data = Data(json.utf8)
        let session = try JSONDecoder.atlas.decode(Session.self, from: data)

        #expect(session.kind == .ssh)
        #expect(session.isActive == true)
        #expect(session.serverId != nil)
    }

    @Test("ServerStatus has correct system images")
    func serverStatusImages() {
        #expect(ServerStatus.online.systemImage == "circle.fill")
        #expect(ServerStatus.offline.systemImage == "circle")
        #expect(ServerStatus.unreachable.systemImage == "exclamationmark.circle")
    }

    @Test("DaemonError descriptions")
    func daemonErrors() {
        let err = DaemonError.notConnected
        #expect(err.errorDescription == "Not connected to daemon")

        let rpcErr = DaemonError.rpcError("method not found")
        #expect(rpcErr.errorDescription == "RPC error: method not found")
    }
}
