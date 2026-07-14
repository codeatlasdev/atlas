import SwiftUI

struct AccountView: View {
    @State private var authService = AuthService()

    var body: some View {
        Form {
            Section("Google Account") {
                if authService.isAuthenticated {
                    HStack {
                        Image(systemName: "person.crop.circle.fill")
                            .font(.title)
                            .foregroundStyle(.atlas(.accent))

                        VStack(alignment: .leading) {
                            Text(authService.userEmail ?? "Connected")
                                .atlasFont(.body)
                            Text("Signed in with Google")
                                .atlasFont(.caption)
                                .foregroundStyle(.textSecondary)
                        }

                        Spacer()

                        Button("Sign Out") {
                            authService.signOut()
                        }
                    }
                } else {
                    VStack(spacing: 12) {
                        Text("Sign in to sync your servers and settings across devices.")
                            .atlasFont(.body)
                            .foregroundStyle(.textSecondary)

                        Button("Sign in with Google") {
                            Task {
                                try? await authService.signIn()
                            }
                        }
                        .controlSize(.large)
                    }
                }
            }
        }
        .formStyle(.grouped)
        .padding()
    }
}
