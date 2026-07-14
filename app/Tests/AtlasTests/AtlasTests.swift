import XCTest
@testable import Atlas

final class AtlasTests: XCTestCase {
    func testServerDecoding() throws {
        let json = """
        {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "prod-1",
            "host": "10.0.0.1",
            "user": "root",
            "port": 22,
            "status": "online",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        }
        """.data(using: .utf8)!

        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let server = try decoder.decode(Server.self, from: json)
        XCTAssertEqual(server.name, "prod-1")
        XCTAssertEqual(server.status, .online)
    }
}
