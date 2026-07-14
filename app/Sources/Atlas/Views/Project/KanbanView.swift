import SwiftUI

struct KanbanView: View {
    @Environment(AppState.self) private var appState
    @State private var showCreateTask = false
    @State private var draggedTask: TaskItem?

    var body: some View {
        VStack(spacing: 0) {
            kanbanHeader
            Divider().background(AtlasColors.border)
            kanbanBoard
        }
        .background(AtlasColors.backgroundDeep)
        .sheet(isPresented: $showCreateTask) {
            CreateTaskSheet()
        }
    }

    // MARK: - Header

    private var kanbanHeader: some View {
        HStack {
            Text("Kanban Board")
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(AtlasColors.textPrimary)

            Spacer()

            Text("\(appState.tasks.count) tasks")
                .atlasFont(.caption)
                .foregroundStyle(AtlasColors.textTertiary)

            Button {
                showCreateTask = true
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "plus")
                    Text("New Task")
                }
                .font(.system(size: 12, weight: .medium))
            }
            .buttonStyle(NeonButtonStyle(color: AtlasColors.neonCyan))
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 14)
        .background(AtlasColors.backgroundSurface.opacity(0.5))
    }

    // MARK: - Board

    private var kanbanBoard: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .top, spacing: 16) {
                ForEach(TaskStatus.allCases, id: \.self) { status in
                    KanbanColumn(
                        status: status,
                        tasks: appState.tasks.filter { $0.status == status },
                        draggedTask: $draggedTask
                    )
                }
            }
            .padding(20)
        }
    }
}

// MARK: - Kanban Column

struct KanbanColumn: View {
    @Environment(AppState.self) private var appState
    let status: TaskStatus
    let tasks: [TaskItem]
    @Binding var draggedTask: TaskItem?
    @State private var isTargeted = false

    var body: some View {
        VStack(spacing: 12) {
            columnHeader
            taskList
        }
        .frame(width: 260)
        .background {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(AtlasColors.backgroundSurface.opacity(isTargeted ? 0.8 : 0.4))
        }
        .overlay {
            if isTargeted {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(status.color.opacity(0.5), lineWidth: 1)
            }
        }
        .dropDestination(for: String.self) { items, _ in
            guard let taskId = items.first,
                  let uuid = UUID(uuidString: taskId) else { return false }
            appState.moveTask(
                TaskItem(id: uuid, title: "", description: "", status: .backlog, priority: .low, assignedAgent: nil, labels: []),
                to: status
            )
            // Find and move the actual task
            if let task = appState.tasks.first(where: { $0.id.uuidString == taskId }) {
                appState.moveTask(task, to: status)
            }
            return true
        } isTargeted: { targeted in
            withAnimation(.easeInOut(duration: 0.2)) {
                isTargeted = targeted
            }
        }
    }

    private var columnHeader: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(status.color)
                .frame(width: 8, height: 8)
                .shadow(color: status.color.opacity(0.5), radius: 3)

            Text(status.rawValue)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(AtlasColors.textPrimary)

            Spacer()

            Text("\(tasks.count)")
                .font(.system(size: 11, weight: .medium, design: .rounded))
                .foregroundStyle(AtlasColors.textTertiary)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background {
                    Capsule()
                        .fill(AtlasColors.backgroundGlass)
                }
        }
        .padding(.horizontal, 14)
        .padding(.top, 14)
        .padding(.bottom, 4)
    }

    private var taskList: some View {
        ScrollView {
            LazyVStack(spacing: 8) {
                ForEach(tasks) { task in
                    KanbanCard(task: task)
                        .draggable(task.id.uuidString)
                }
            }
            .padding(.horizontal, 10)
            .padding(.bottom, 14)
        }
    }
}

// MARK: - Kanban Card

struct KanbanCard: View {
    let task: TaskItem
    @State private var isHovered = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Title
            Text(task.title)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(AtlasColors.textPrimary)
                .lineLimit(2)

            // Description
            if !task.description.isEmpty {
                Text(task.description)
                    .font(.system(size: 11))
                    .foregroundStyle(AtlasColors.textTertiary)
                    .lineLimit(2)
            }

            // Footer: priority + agent + labels
            HStack(spacing: 6) {
                // Priority
                Text(task.priority.rawValue)
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(task.priority.color)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background {
                        Capsule()
                            .fill(task.priority.color.opacity(0.15))
                    }

                // Labels
                ForEach(task.labels.prefix(2), id: \.self) { label in
                    Text(label)
                        .font(.system(size: 10))
                        .foregroundStyle(AtlasColors.textSecondary)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 2)
                        .background {
                            Capsule()
                                .fill(AtlasColors.backgroundGlass)
                        }
                }

                Spacer()

                // Agent
                if let agent = task.assignedAgent {
                    HStack(spacing: 3) {
                        Image(systemName: "cpu")
                            .font(.system(size: 9))
                        Text(agent)
                            .font(.system(size: 10))
                    }
                    .foregroundStyle(AtlasColors.neonCyan)
                }
            }
        }
        .padding(12)
        .background {
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(AtlasColors.backgroundElevated)
        }
        .overlay {
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(
                    isHovered ? AtlasColors.neonCyan.opacity(0.3) : AtlasColors.border,
                    lineWidth: 0.5
                )
        }
        .shadow(
            color: isHovered ? AtlasColors.neonCyan.opacity(0.1) : .clear,
            radius: 8, x: 0, y: 2
        )
        .onHover { hovering in
            withAnimation(.easeInOut(duration: 0.15)) {
                isHovered = hovering
            }
        }
    }
}

// MARK: - Create Task Sheet

struct CreateTaskSheet: View {
    @Environment(AppState.self) private var appState
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var description = ""
    @State private var priority: TaskPriority = .medium

    var body: some View {
        VStack(spacing: 20) {
            Text("New Task")
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(AtlasColors.textPrimary)

            VStack(spacing: 12) {
                TextField("Task title", text: $title)
                    .textFieldStyle(.plain)
                    .padding(10)
                    .background(AtlasColors.backgroundElevated)
                    .clipShape(RoundedRectangle(cornerRadius: 8))

                TextField("Description (optional)", text: $description, axis: .vertical)
                    .textFieldStyle(.plain)
                    .lineLimit(3...5)
                    .padding(10)
                    .background(AtlasColors.backgroundElevated)
                    .clipShape(RoundedRectangle(cornerRadius: 8))

                Picker("Priority", selection: $priority) {
                    ForEach(TaskPriority.allCases, id: \.self) { p in
                        Text(p.rawValue).tag(p)
                    }
                }
                .pickerStyle(.segmented)
            }

            HStack {
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)

                Spacer()

                Button("Create") {
                    appState.createTask(title: title, description: description, priority: priority)
                    dismiss()
                }
                .buttonStyle(GradientButtonStyle())
                .disabled(title.isEmpty)
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(width: 400)
        .background(AtlasColors.backgroundSurface)
    }
}
