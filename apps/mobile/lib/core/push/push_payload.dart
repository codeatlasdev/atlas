/// What a push says. The relay forwards the FCM `data` hostd built: always `kind`; for a plain
/// agent's push also `pane` (the agent's pane key on that computer) and a short `title`
/// (`<agent> · <project folder>`). Never conversation text.
enum PushKind {
  /// A crewmate finished a task.
  taskDone('task_done'),

  /// A crewmate needs you.
  needsYou('needs_you'),

  /// A plain agent (omp, Claude Code, Codex) finished.
  agentDone('agent_done'),

  /// A plain agent waits for an approval or an answer.
  agentNeedsYou('agent_needs_you');

  const PushKind(this.wire);
  final String wire;

  static PushKind? parse(Object? v) {
    for (final k in values) {
      if (k.wire == v) return k;
    }
    return null;
  }
}

class PushPayload {
  const PushPayload({required this.kind, this.pane, this.title = ''});

  final PushKind kind;

  /// The agent's pane key; set for the agent kinds only.
  final String? pane;
  final String title;

  /// Whether tapping it opens an agent's chat (and so needs a pane).
  bool get opensAgent => pane != null;

  /// Parses FCM `data`. `null` for anything this app cannot act on: an unknown kind (a newer
  /// hostd), no kind, or an agent push without a pane.
  static PushPayload? fromData(Map<String, dynamic> data) {
    final kind = PushKind.parse(data['kind']);
    if (kind == null) return null;
    final title = data['title'] is String ? data['title'] as String : '';
    final isAgent =
        kind == PushKind.agentDone || kind == PushKind.agentNeedsYou;
    if (!isAgent) return PushPayload(kind: kind, title: title);
    final pane = data['pane'];
    if (pane is! String || pane.isEmpty) return null;
    return PushPayload(kind: kind, pane: pane, title: title);
  }
}
