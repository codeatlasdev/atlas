import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/todo_list.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/tree/strip_decoration.dart';

/// The main agent's todo list (omp's todo tool): collapsed to the task in progress and a
/// count, expands to the phases with their tasks ticked.
class TodoCard extends StatefulWidget {
  const TodoCard(this.todos, {super.key});
  final TodoList todos;

  @override
  State<TodoCard> createState() => _TodoCardState();
}

class _TodoCardState extends State<TodoCard> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = widget.todos;
    final current = t.current;
    return Container(
      width: double.infinity,
      decoration: stripDecoration(c),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InkWell(
            key: const ValueKey('todo-toggle'),
            onTap: () => setState(() => _open = !_open),
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: Space.lg,
                vertical: Space.sm,
              ),
              child: Row(
                children: [
                  Icon(Icons.checklist_rounded, size: 16, color: c.muted),
                  const SizedBox(width: Space.sm),
                  Text(
                    '${t.done}/${t.total}',
                    style: Theme.of(context).textTheme.bodySmall!
                        .copyWith(fontWeight: FontWeight.w700, color: c.text),
                  ),
                  const SizedBox(width: Space.sm),
                  Expanded(
                    child: Text(
                      current?.content ?? 'Todos',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.bodySmall!
                          .copyWith(color: c.muted),
                    ),
                  ),
                  Icon(
                    _open ? Icons.expand_less : Icons.expand_more,
                    size: 18,
                    color: c.faint,
                  ),
                ],
              ),
            ),
          ),
          if (_open)
            ConstrainedBox(
              constraints: BoxConstraints(
                maxHeight: MediaQuery.sizeOf(context).height * .35,
              ),
              child: SingleChildScrollView(
                padding: const EdgeInsets.fromLTRB(
                  Space.lg,
                  0,
                  Space.lg,
                  Space.sm,
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    for (final p in t.phases) ...[
                      Padding(
                        padding: const EdgeInsets.only(top: Space.xs),
                        child: Text(
                          p.name,
                          style: Theme.of(context).textTheme.bodySmall!
                              .copyWith(
                                fontWeight: FontWeight.w700,
                                color: c.muted,
                              ),
                        ),
                      ),
                      for (final k in p.tasks) _TodoRow(k),
                    ],
                  ],
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class _TodoRow extends StatelessWidget {
  const _TodoRow(this.task);
  final TodoTask task;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (icon, color) = switch (task.status) {
      'completed' => (Icons.check_box_rounded, c.success),
      'in_progress' => (Icons.timelapse_rounded, c.accent),
      'blocked' => (Icons.block_rounded, c.error),
      'abandoned' => (Icons.indeterminate_check_box_outlined, c.faint),
      _ => (Icons.check_box_outline_blank_rounded, c.faint),
    };
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, size: 16, color: color),
          const SizedBox(width: Space.sm),
          Expanded(
            child: Text(
              task.content,
              style: Theme.of(context).textTheme.bodySmall!.copyWith(
                height: 1.3,
                color: task.done || task.status == 'abandoned'
                    ? c.faint
                    : c.text,
                decoration: task.done ? TextDecoration.lineThrough : null,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
