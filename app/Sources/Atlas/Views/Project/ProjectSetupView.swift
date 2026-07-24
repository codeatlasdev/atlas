import SwiftUI

// MARK: - Detection Result Model

struct ProjectDetectionResult: Hashable {
    var language: String
    var framework: String?
    var packageManager: String?
    var scripts: [String: String]
    var services: [DetectedServiceItem]
    var deployStrategy: String?
    var monorepo: Bool

    static func from(_ dict: [String: Any]) -> Self {
        let scripts = dict["scripts"] as? [String: String] ?? [:]
        let servicesRaw = dict["services"] as? [[String: Any]] ?? []
        let services = servicesRaw.map { s in
            DetectedServiceItem(
                name: s["name"] as? String ?? "",
                command: s["command"] as? String ?? "",
                port: s["port"] as? Int,
                devCommand: s["dev_command"] as? String
            )
        }
        return ProjectDetectionResult(
            language: dict["language"] as? String ?? "unknown",
            framework: dict["framework"] as? String,
            packageManager: dict["package_manager"] as? String,
            scripts: scripts,
            services: services,
            deployStrategy: dict["deploy_strategy"] as? String,
            monorepo: dict["monorepo"] as? Bool ?? false
        )
    }
}

struct DetectedServiceItem: Identifiable, Hashable {
    let id = UUID()
    var name: String
    var command: String
    var port: Int?
    var devCommand: String?
}

// MARK: - Setup Wizard

struct ProjectSetupView: View {
    @Environment(AppState.self) private var appState
    @State private var step: SetupStep = .detecting
    @State private var detection: ProjectDetectionResult?
    @State private var editedServices: [DetectedServiceItem] = []
    @State private var editedDeploy: String = "systemd"
    @State private var projectName: String = ""
    @State private var yamlPreview: String = ""
    @State private var error: String?
    @State private var isGenerating = false

    enum SetupStep: Int, CaseIterable {
        case detecting = 0
        case review = 1
        case configure = 2
        case preview = 3
        case done = 4
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            stepIndicator
            Divider().background(AtlasColors.border)

            ScrollView {
                VStack(spacing: 24) {
                    stepContent
                }
                .padding(32)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(AtlasColors.backgroundDeep)
        .task {
            await runDetection()
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: 14) {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [AtlasColors.neonCyan, AtlasColors.neonPurple],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .frame(width: 40, height: 40)
                .overlay {
                    Image(systemName: "wand.and.stars")
                        .font(.system(size: 18, weight: .medium))
                        .foregroundStyle(.white)
                }

            VStack(alignment: .leading, spacing: 2) {
                Text("Project Setup")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(AtlasColors.textPrimary)

                Text("Configure your workspace for Atlas")
                    .font(.system(size: 12))
                    .foregroundStyle(AtlasColors.textTertiary)
            }

            Spacer()

            Button {
                appState.closeProject()
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 20))
                    .foregroundStyle(AtlasColors.textTertiary)
            }
            .buttonStyle(.plain)
            .help("Cancel Setup")
        }
        .padding(.horizontal, 32)
        .padding(.vertical, 20)
    }

    // MARK: - Step Indicator

    private var stepIndicator: some View {
        HStack(spacing: 0) {
            ForEach(Array(SetupStep.allCases.prefix(4).enumerated()), id: \.offset) { index, s in
                HStack(spacing: 8) {
                    Circle()
                        .fill(stepColor(for: s))
                        .frame(width: 8, height: 8)
                        .shadow(color: stepColor(for: s).opacity(0.5), radius: 3)

                    Text(stepLabel(for: s))
                        .font(.system(size: 11, weight: step.rawValue >= s.rawValue ? .medium : .regular))
                        .foregroundStyle(step.rawValue >= s.rawValue ? AtlasColors.textPrimary : AtlasColors.textTertiary)
                }

                if index < 3 {
                    Rectangle()
                        .fill(step.rawValue > s.rawValue ? AtlasColors.neonCyan.opacity(0.5) : AtlasColors.border)
                        .frame(height: 1)
                        .frame(maxWidth: .infinity)
                        .padding(.horizontal, 8)
                }
            }
        }
        .padding(.horizontal, 32)
        .padding(.vertical, 14)
        .background(AtlasColors.backgroundSurface.opacity(0.5))
    }

    // MARK: - Step Content

    @ViewBuilder
    private var stepContent: some View {
        switch step {
        case .detecting:
            detectingView
        case .review:
            reviewView
        case .configure:
            configureView
        case .preview:
            previewView
        case .done:
            doneView
        }
    }

    // MARK: - Step 1: Detecting

    private var detectingView: some View {
        VStack(spacing: 20) {
            Spacer().frame(height: 40)

            ProgressView()
                .controlSize(.large)
                .tint(AtlasColors.neonCyan)

            Text("Analyzing project...")
                .atlasFont(.headline)
                .foregroundStyle(AtlasColors.textPrimary)

            Text("Scanning files, dependencies, and configuration")
                .atlasFont(.body)
                .foregroundStyle(AtlasColors.textSecondary)

            if let error {
                Text(error)
                    .atlasFont(.caption)
                    .foregroundStyle(AtlasColors.neonRed)
                    .padding(.top, 8)

                Button("Retry") {
                    self.error = nil
                    Task { await runDetection() }
                }
                .buttonStyle(NeonButtonStyle(color: AtlasColors.neonCyan))
            }

            Spacer().frame(height: 40)
        }
        .frame(maxWidth: .infinity)
    }

    // MARK: - Step 2: Review

    private var reviewView: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("We detected the following about your project:")
                .atlasFont(.body)
                .foregroundStyle(AtlasColors.textSecondary)

            if let detection {
                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 12) {
                    detectionCard(icon: "chevron.left.forwardslash.chevron.right", label: "Language", value: detection.language)
                    detectionCard(icon: "cube", label: "Framework", value: detection.framework ?? "None")
                    detectionCard(icon: "shippingbox", label: "Package Manager", value: detection.packageManager ?? "None")
                    detectionCard(icon: "arrow.triangle.branch", label: "Monorepo", value: detection.monorepo ? "Yes" : "No")
                    detectionCard(icon: "paperplane.fill", label: "Deploy Strategy", value: detection.deployStrategy ?? "systemd")
                    detectionCard(icon: "list.bullet", label: "Scripts Found", value: "\(detection.scripts.count)")
                }

                if !detection.services.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Detected Services")
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(AtlasColors.textPrimary)

                        ForEach(detection.services) { service in
                            serviceRow(service)
                        }
                    }
                    .padding(.top, 8)
                }
            }

            HStack {
                Spacer()
                Button("Continue") {
                    prepareConfigStep()
                    withAnimation(.spring(duration: 0.3)) { step = .configure }
                }
                .buttonStyle(GradientButtonStyle())
            }
            .padding(.top, 12)
        }
    }

    // MARK: - Step 3: Configure

    private var configureView: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Adjust the configuration as needed:")
                .atlasFont(.body)
                .foregroundStyle(AtlasColors.textSecondary)

            // Project name
            VStack(alignment: .leading, spacing: 6) {
                Text("Project Name")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(AtlasColors.textSecondary)

                TextField("my-project", text: $projectName)
                    .textFieldStyle(.plain)
                    .font(.system(size: 14, design: .monospaced))
                    .padding(10)
                    .background(AtlasColors.backgroundElevated)
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .overlay {
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .strokeBorder(AtlasColors.border, lineWidth: 0.5)
                    }
            }

            // Deploy strategy
            VStack(alignment: .leading, spacing: 6) {
                Text("Deploy Strategy")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(AtlasColors.textSecondary)

                Picker("", selection: $editedDeploy) {
                    Text("Docker").tag("docker")
                    Text("systemd").tag("systemd")
                    Text("Fly.io").tag("fly")
                    Text("Vercel").tag("vercel")
                    Text("Netlify").tag("netlify")
                }
                .pickerStyle(.segmented)
            }

            // Services
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("Services")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(AtlasColors.textSecondary)

                    Spacer()

                    Button {
                        editedServices.append(DetectedServiceItem(name: "new-service", command: "", port: nil, devCommand: nil))
                    } label: {
                        Image(systemName: "plus.circle")
                            .font(.system(size: 14))
                            .foregroundStyle(AtlasColors.neonCyan)
                    }
                    .buttonStyle(.plain)
                }

                ForEach($editedServices) { $service in
                    editableServiceRow(service: $service)
                }

                if editedServices.isEmpty {
                    Text("No services configured. Add one or continue without.")
                        .atlasFont(.caption)
                        .foregroundStyle(AtlasColors.textTertiary)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                }
            }

            HStack {
                Button("Back") {
                    withAnimation(.spring(duration: 0.3)) { step = .review }
                }
                .buttonStyle(NeonButtonStyle(color: AtlasColors.textTertiary))

                Spacer()

                Button("Preview YAML") {
                    generatePreview()
                    withAnimation(.spring(duration: 0.3)) { step = .preview }
                }
                .buttonStyle(GradientButtonStyle())
            }
            .padding(.top, 12)
        }
    }

    // MARK: - Step 4: Preview

    private var previewView: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Your atlas.yaml")
                .atlasFont(.headline)
                .foregroundStyle(AtlasColors.textPrimary)

            Text("This file will be created at the root of your project.")
                .atlasFont(.body)
                .foregroundStyle(AtlasColors.textSecondary)

            // YAML preview
            ScrollView(.horizontal, showsIndicators: false) {
                Text(yamlPreview)
                    .font(.system(size: 13, design: .monospaced))
                    .foregroundStyle(AtlasColors.neonGreen)
                    .padding(16)
            }
            .frame(maxWidth: .infinity, minHeight: 200, alignment: .topLeading)
            .background(AtlasColors.backgroundElevated)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(AtlasColors.neonGreen.opacity(0.2), lineWidth: 0.5)
            }

            HStack {
                Button("Back") {
                    withAnimation(.spring(duration: 0.3)) { step = .configure }
                }
                .buttonStyle(NeonButtonStyle(color: AtlasColors.textTertiary))

                Spacer()

                Button {
                    Task { await createYaml() }
                } label: {
                    HStack(spacing: 6) {
                        if isGenerating {
                            ProgressView()
                                .controlSize(.small)
                                .tint(.white)
                        } else {
                            Image(systemName: "doc.badge.plus")
                        }
                        Text("Create atlas.yaml")
                    }
                }
                .buttonStyle(GradientButtonStyle())
                .disabled(isGenerating)
            }
            .padding(.top, 12)
        }
    }

    // MARK: - Step 5: Done

    private var doneView: some View {
        VStack(spacing: 20) {
            Spacer().frame(height: 40)

            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 52))
                .foregroundStyle(AtlasColors.neonGreen)
                .shadow(color: AtlasColors.neonGreen.opacity(0.4), radius: 12)

            Text("Project configured!")
                .atlasFont(.title)
                .foregroundStyle(AtlasColors.textPrimary)

            Text("atlas.yaml has been created. Your workspace is ready.")
                .atlasFont(.body)
                .foregroundStyle(AtlasColors.textSecondary)
                .multilineTextAlignment(.center)

            Button("Open Workspace") {
                appState.needsProjectSetup = false
            }
            .buttonStyle(GradientButtonStyle())
            .padding(.top, 8)

            Spacer().frame(height: 40)
        }
        .frame(maxWidth: .infinity)
    }

    // MARK: - Helpers

    private func detectionCard(icon: String, label: String, value: String) -> some View {
        HStack(spacing: 10) {
            Image(systemName: icon)
                .font(.system(size: 14))
                .foregroundStyle(AtlasColors.neonCyan)
                .frame(width: 24)

            VStack(alignment: .leading, spacing: 2) {
                Text(label)
                    .font(.system(size: 11))
                    .foregroundStyle(AtlasColors.textTertiary)
                Text(value)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(AtlasColors.textPrimary)
            }
            Spacer()
        }
        .padding(12)
        .background(AtlasColors.backgroundElevated)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(AtlasColors.border, lineWidth: 0.5)
        }
    }

    private func serviceRow(_ service: DetectedServiceItem) -> some View {
        HStack(spacing: 10) {
            Circle()
                .fill(AtlasColors.neonGreen)
                .frame(width: 6, height: 6)

            Text(service.name)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(AtlasColors.textPrimary)

            if let port = service.port {
                Text(":\(port)")
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(AtlasColors.neonCyan)
            }

            Spacer()

            Text(service.command)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(AtlasColors.textTertiary)
                .lineLimit(1)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(AtlasColors.backgroundElevated)
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }

    private func editableServiceRow(service: Binding<DetectedServiceItem>) -> some View {
        HStack(spacing: 8) {
            TextField("name", text: service.name)
                .textFieldStyle(.plain)
                .font(.system(size: 13, design: .monospaced))
                .frame(width: 100)

            TextField("command", text: service.command)
                .textFieldStyle(.plain)
                .font(.system(size: 13, design: .monospaced))

            TextField("port", value: Binding(
                get: { service.wrappedValue.port ?? 0 },
                set: { service.wrappedValue.port = $0 == 0 ? nil : $0 }
            ), format: .number)
            .textFieldStyle(.plain)
            .font(.system(size: 13, design: .monospaced))
            .frame(width: 60)

            Button {
                editedServices.removeAll { $0.id == service.wrappedValue.id }
            } label: {
                Image(systemName: "minus.circle")
                    .foregroundStyle(AtlasColors.neonRed)
            }
            .buttonStyle(.plain)
        }
        .padding(10)
        .background(AtlasColors.backgroundElevated)
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .strokeBorder(AtlasColors.border, lineWidth: 0.5)
        }
    }

    private func stepColor(for s: SetupStep) -> Color {
        if step.rawValue > s.rawValue { return AtlasColors.neonGreen }
        if step == s { return AtlasColors.neonCyan }
        return AtlasColors.textTertiary
    }

    private func stepLabel(for s: SetupStep) -> String {
        switch s {
        case .detecting: "Detect"
        case .review: "Review"
        case .configure: "Configure"
        case .preview: "Confirm"
        case .done: "Done"
        }
    }

    // MARK: - Actions

    private func runDetection() async {
        guard let project = appState.currentProject else { return }

        do {
            let response = try await appState.daemon.send(method: "project.detect", params: [
                "path": project.path
            ])
            if let dict = response as? [String: Any] {
                let result = ProjectDetectionResult.from(dict)
                detection = result
                appState.projectDetection = result
                projectName = project.name
                editedDeploy = result.deployStrategy ?? "systemd"

                withAnimation(.spring(duration: 0.3)) { step = .review }
            } else {
                error = "Unexpected response from daemon"
            }
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func prepareConfigStep() {
        guard let detection else { return }
        editedServices = detection.services
    }

    private func generatePreview() {
        var lines: [String] = []
        lines.append("name: \(projectName)")

        if !editedServices.isEmpty {
            lines.append("services:")
            for service in editedServices {
                lines.append("  \(service.name):")
                lines.append("    command: \"\(service.command)\"")
                if let port = service.port {
                    lines.append("    port: \(port)")
                }
            }
        }

        lines.append("deploy:")
        lines.append("  strategy: \(editedDeploy)")

        yamlPreview = lines.joined(separator: "\n")
    }

    private func createYaml() async {
        guard let project = appState.currentProject else { return }
        isGenerating = true

        var servicesDict: [String: [String: Any]] = [:]
        for service in editedServices {
            var svc: [String: Any] = ["command": service.command]
            if let port = service.port {
                svc["port"] = port
            }
            servicesDict[service.name] = svc
        }

        let config: [String: Any] = [
            "name": projectName,
            "services": servicesDict,
            "deploy": ["strategy": editedDeploy]
        ]

        do {
            try await appState.daemon.send(method: "project.generate_yaml", params: [
                "path": project.path,
                "config": config
            ])
            withAnimation(.spring(duration: 0.3)) { step = .done }
        } catch {
            self.error = error.localizedDescription
        }

        isGenerating = false
    }
}
