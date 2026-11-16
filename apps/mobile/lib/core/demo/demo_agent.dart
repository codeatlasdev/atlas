/// One agent of the demo computer: its conversation, steps and subagents, and the status
/// hostd would push for it.
library;

import 'package:atlas_mobile/core/demo/demo_bus.dart';

class DemoAgent {
  DemoAgent(
    this.pane,
    this.agent,
    this.state,
    this.history, {
    this.title,
    this.steps = const [],
    this.subagents = const [],
  });
  final String pane;
  final String agent;
  String state;
  final List<Map<String, String>> history;
  String? title;

  /// Tool calls and subagent calls shown between the first prompt and the answer, in
  /// hostd's `agent_history` item shape.
  final List<Map<String, Object?>> steps;
  final List<Map<String, Object?>> subagents;
  int updatedAt = demoNow();
  int startedAt = demoNow();

  /// `agent_history`'s `items`: the conversation plus its tool steps. The steps sit
  /// before the last assistant message, like a real turn.
  List<Map<String, Object?>> items() {
    final msgs = [
      for (final m in history) {'kind': m['role'], 'text': m['text']},
    ];
    if (steps.isEmpty || msgs.isEmpty) return msgs;
    final at = msgs.lastIndexWhere((m) => m['kind'] == 'user') + 1;
    return [
      ...msgs.take(at),
      {'kind': 'thinking', 'text': 'Look at the code before changing it.'},
      ...steps,
      ...msgs.skip(at),
    ];
  }

  Map<String, Object?> status() => {
    'state': state,
    'agent': agent,
    'session_id': null,
    'session_file': null,
    'model': switch (agent) {
      'claude' => 'claude-sonnet',
      'omp' => 'gpt-5.5',
      _ => null,
    },
    'prompt': history.lastWhere(
      (m) => m['role'] == 'user',
      orElse: () => const {},
    )['text'],
    'last_message': state == 'working'
        ? null
        : history.lastWhere(
            (m) => m['role'] == 'assistant',
            orElse: () => const {},
          )['text'],
    'updated_at': updatedAt,
    'state_started_at': startedAt,
    'worktree_id': null,
    'tab_id': null,
    'title': title,
  };
}
