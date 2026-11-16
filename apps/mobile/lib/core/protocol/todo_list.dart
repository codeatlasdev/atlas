/// The root agent's todo list (omp's todo tool).
library;

/// A todo of the root agent's list; [status]: `pending` | `in_progress` | `completed` |
/// `abandoned` | `blocked`.
class TodoTask {
  const TodoTask({required this.content, required this.status});
  final String content;
  final String status;
  bool get done => status == 'completed';
  bool get active => status == 'in_progress';
}

class TodoPhase {
  const TodoPhase({required this.name, required this.tasks});
  final String name;
  final List<TodoTask> tasks;
}

/// The root agent's todo list (omp's todo tool), as phases of tasks.
class TodoList {
  const TodoList({required this.phases, this.done = 0, this.total = 0});
  final List<TodoPhase> phases;
  final int done;
  final int total;

  /// The task omp is on now, if any.
  TodoTask? get current {
    for (final p in phases) {
      for (final t in p.tasks) {
        if (t.active) return t;
      }
    }
    return null;
  }

  static TodoList? fromJson(Object? j) {
    if (j is! Map) return null;
    final phases = [
      for (final p in (j['phases'] as List?) ?? const [])
        TodoPhase(
          name: (p as Map)['name'] as String? ?? '',
          tasks: [
            for (final t in (p['tasks'] as List?) ?? const [])
              TodoTask(
                content: (t as Map)['content'] as String? ?? '',
                status: t['status'] as String? ?? 'pending',
              ),
          ],
        ),
    ];
    return TodoList(
      phases: phases,
      done: (j['done'] as num?)?.toInt() ?? 0,
      total: (j['total'] as num?)?.toInt() ?? 0,
    );
  }
}
