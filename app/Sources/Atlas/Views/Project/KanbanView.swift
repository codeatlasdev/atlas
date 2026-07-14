import SwiftUI

struct KanbanView: View {
    @Environment(AppState.self) private var appState
    @State private var showCreateTask = false
    @State private var draggedTask: TaskItem?

    var body: some View {
        VStack(spacing: 0) {
            kanbanHeader
            SoftDivider()
            kanbanBoard
        }
        .background(DS.bg.base)
        .sheet(isPresented: $showCreateTask) {
            CreateTaskSheet()
        }
    }

    // MARK: - Header

    private var kanbanHeader: some View {
        HStack {
            Text("Kanban Board")
                .font(.atlasTitle)
                .foregroundStyle(DS.text.primary)

            Spacer()

            Text("\(appState.tasks.count) tasks")
                .font(.atlasCaption)
                .foregroundStyle(DS.text.tertiary)

            AtlasButton("New Task", icon: "plus") {
                showCreateTask = true
            }
        }
        .padding(.horizontal, DS.spacing.xxl)
        .padding(.vertical, 14)
        .background(DS.bg.elevated.opacity(0.5))
    }

    // MARK: - Board

    private var kanbanBoard: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .top, spacing: DS.spacing.lg) {
                ForEach(TaskStatus.allCases, id: \.self) { status in
                    KanbanColumn(
                        status: status,
                        tasks: appState.tasks.filter { $0.status == status },
                        draggedTask: $draggedTask
                    )
                }
            }
            .padding(DS.spacing.xl)
        }
    }
}

// MARK: - Column

struct KanbanColumn: View {
    @Environment(AppState.self) private var appState
    let status: TaskStatus
    let tasks: [TaskItem]
    @Binding var draggedTask: TaskItem?
    @State private var isTargeted = false

    var body: some View {
        VStack(spacing: DS.spacing.md) {
            columnHeader
            taskList
        }
        .frame(width: 260)
        .background(
            RoundedRectangle(cornerRadius: DS.radius.lg, style: .continuous)
                .fill(DS.bg.elevated.opacity(isTargeted ? 0.8 : 0.4))
        )
        .overlay {
            if isTargeted {
                RoundedRectangle(cornerRadius: DS.radius.lg, style: .continuous)
                    .strokeBorder(DS.accent.primary.opacity(0.3), lineWidth: 1)
            }
        }
        .dropDestination(for: String.self) { items, _ in
            guard let taskId = items.first,
                  let uuid = UUID(uuidString: taskId) else { return false }
            appState.moveTask(
                TaskItem(id: uuid, title: "", description: "", status: .backlog, priority: .low, assignedAgent: nil, labels: []),
                to: status
            )
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
        HStack(spacing: DS.spacing.sm) {
            Circle()
                .fill(status.color)
                .frame(width: 8, height: 8)

            Text(status.rawValue)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(DS.text.primary)

            Spacer()

            Text("\(tasks.count)")
                .font(.system(size: 11, weight: .medium, design: .rounded))
                .foregroundStyle(DS.text.tertiary)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(DS.bg.hover)
                .clipShape(Capsule())
        }
        .padding(.horizontal, 14)
        .padding(.top, 14)
        .padding(.bottom, DS.spacing.xs)
    }

    private var taskList: some View {
        ScrollView {
            LazyVStack(spacing: DS.spacing.sm) {
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

// MARK: - Card

struct KanbanCard: View {
    let task: TaskItem
    @State private var isHovered = false

    var body: some View {
        VStack(alignment: .leading, spacing: DS.spacing.sm) {
            Text(task.title)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(DS.text.primary)
                .lineLimit(2)

            if !task.description.isEmpty {
                Text(task.description)
                    .font(.system(size: 11))
                    .foregroundStyle(DS.text.tertiary)
                    .lineLimit(2)
            }

            HStack(spacing: 6) {
                Badge(text: task.priority.rawValue, color: task.priority.color, size: .small)

                ForEach(task.labels.prefix(2), id: \.self) { label in
                    Text(label)
                        .font(.system(size: 10))
                        .foregroundStyle(DS.text.secondary)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 2)
                        .background(DS.bg.hover)
                        .clipShape(Capsule())
                }

                Spacer()

                if let agent = task.assignedAgent {
                    HStack(spacing: 3) {
                        Image(systemName: "cpu")
                            .font(.system(size: 9))
                        Text(agent)
                            .font(.system(size: 10))
                    }
                    .foregroundStyle(DS.accent.primary)
                }
            }
        }
        .padding(DS.spacing.md)
        .background(DS.bg.elevated)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(
                    isHovered ? DS.accent.primary.opacity(0.2) : DS.border.subtle,
                    lineWidth: 0.5
                )
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
        VStack(spacing: DS.spacing.xl) {
            Text("New Task")
                .font(.atlasTitle)
                .foregroundStyle(DS.text.primary)

            VStack(spacing: DS.spacing.md) {
                TextField("Task title", text: $title)
                    .textFieldStyle(.plain)
                    .font(.atlasBody)
                    .padding(10)
                    .background(DS.bg.elevated2)
                    .clipShape(RoundedRectangle(cornerRadius: DS.radius.md))
                    .overlay(
                        RoundedRectangle(cornerRadius: DS.radius.md)
                            .strokeBorder(DS.border.medium, lineWidth: 0.5)
                    )

                TextField("Description (optional)", text: $description, axis: .vertical)
                    .textFieldStyle(.plain)
                    .font(.atlasBody)
                    .lineLimit(3...5)
                    .padding(10)
                    .background(DS.bg.elevated2)
                    .clipShape(RoundedRectangle(cornerRadius: DS.radius.md))
                    .overlay(
                        RoundedRectangle(cornerRadius: DS.radius.md)
                            .strokeBorder(DS.border.medium, lineWidth: 0.5)
                    )

                Picker("Priority", selection: $priority) {
                    ForEach(TaskPriority.allCases, id: \.self) { p in
                        Text(p.rawValue).tag(p)
                    }
                }
                .pickerStyle(.segmented)
            }

            HStack {
                Button("Cancel") { dismiss() }
                    .buttonStyle(.plain)
                    .foregroundStyle(DS.text.secondary)
                    .keyboardShortcut(.cancelAction)

                Spacer()

                AtlasButton("Create") {
                    appState.createTask(title: title, description: description, priority: priority)
                    dismiss()
                }
                .disabled(title.isEmpty)
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(DS.spacing.xxl)
        .frame(width: 400)
        .background(DS.bg.elevated)
    }
}
