import SwiftUI

struct SettingsView: View {
    var body: some View {
        TabView {
            GeneralSettingsView()
                .tabItem {
                    Label("General", systemImage: "gear")
                }

            AccountView()
                .tabItem {
                    Label("Account", systemImage: "person.crop.circle")
                }
        }
        .frame(width: 450, height: 300)
    }
}

struct GeneralSettingsView: View {
    @AppStorage("daemonSocketPath") private var socketPath = "~/.atlas/atlas.sock"
    @AppStorage("launchAtLogin") private var launchAtLogin = false
    @AppStorage("showMenuBarExtra") private var showMenuBarExtra = true

    var body: some View {
        Form {
            Section("Daemon") {
                TextField("Socket path:", text: $socketPath)
                    .atlasFont(.mono)
            }

            Section("General") {
                Toggle("Launch at login", isOn: $launchAtLogin)
                Toggle("Show in menu bar", isOn: $showMenuBarExtra)
            }
        }
        .formStyle(.grouped)
        .padding()
    }
}
