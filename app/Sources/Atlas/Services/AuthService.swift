import AuthenticationServices
import Foundation

@Observable
final class AuthService {
    var isAuthenticated = false
    var userEmail: String?

    private var session: ASWebAuthenticationSession?

    func signIn() async throws {
        let callbackScheme = "atlas"
        let authURL = URL(string: "https://accounts.google.com/o/oauth2/v2/auth?client_id=PLACEHOLDER&redirect_uri=atlas://callback&response_type=code&scope=email+profile")!

        let callbackURL = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<URL, Error>) in
            let session = ASWebAuthenticationSession(
                url: authURL,
                callbackURLScheme: callbackScheme
            ) { url, error in
                if let error {
                    continuation.resume(throwing: error)
                } else if let url {
                    continuation.resume(returning: url)
                } else {
                    continuation.resume(throwing: AuthError.cancelled)
                }
            }
            self.session = session
            session.prefersEphemeralWebBrowserSession = false
            session.start()
        }

        if let code = URLComponents(url: callbackURL, resolvingAgainstBaseURL: false)?
            .queryItems?.first(where: { $0.name == "code" })?.value {
            // Exchange code for token via daemon
            _ = code
            isAuthenticated = true
        }
    }

    func signOut() {
        isAuthenticated = false
        userEmail = nil
    }
}

enum AuthError: LocalizedError {
    case cancelled
    case invalidResponse

    var errorDescription: String? {
        switch self {
        case .cancelled: "Authentication was cancelled"
        case .invalidResponse: "Invalid authentication response"
        }
    }
}
