/// The items of an agent conversation timeline (`agent_history`).
library;

sealed class TimelineItem {
  const TimelineItem();

  /// Null for a kind this app does not know (a newer hostd), so it is skipped.
  static TimelineItem? fromJson(Map<String, dynamic> j) => switch (j['kind']) {
    'user' => UserItem(j['text'] as String? ?? ''),
    'assistant' => AssistantItem(j['text'] as String? ?? ''),
    'thinking' => ThinkingItem(j['text'] as String? ?? ''),
    'tool' => ToolItem.fromJson(j),
    'result' => ResultItem.fromJson(j),
    'compaction' => CompactionItem(j['text'] as String? ?? ''),
    'irc' => IrcItem(
      from: j['from'] as String? ?? '',
      agent: j['agent'] as String? ?? '',
      text: j['text'] as String? ?? '',
    ),
    'notice' => NoticeItem(j['text'] as String? ?? ''),
    'advisor' => AdvisorItem(
      severity: j['severity'] as String? ?? 'note',
      text: j['text'] as String? ?? '',
    ),
    _ => null,
  };
}

/// A message another agent sent this one (omp's IRC): [from] is `parent` or an agent
/// name, [agent] the sender's display name (`Main`). Never something the user typed.
class IrcItem extends TimelineItem {
  const IrcItem({required this.from, required this.agent, required this.text});
  final String from;
  final String agent;
  final String text;

  /// Who to show: the sender's name, else its role.
  String get sender => agent.isNotEmpty ? agent : from;
}

/// Something the harness injected into the conversation (a system notice), not a turn.
class NoticeItem extends TimelineItem {
  const NoticeItem(this.text);
  final String text;
}

/// omp's advisor: other models reviewing the agent. [severity]: `blocker` | `concern` |
/// `nit` | `note`.
class AdvisorItem extends TimelineItem {
  const AdvisorItem({required this.severity, required this.text});
  final String severity;
  final String text;
}

/// A subagent finished (omp's `async-result`): [id] is its dotted name, [status]
/// `completed` | `failed` | `aborted`, [duration] hostd's text (`27m20s`).
class ResultItem extends TimelineItem {
  const ResultItem({required this.id, required this.status, this.duration});
  final String id;
  final String status;
  final String? duration;

  factory ResultItem.fromJson(Map<String, dynamic> j) => ResultItem(
    id: j['id'] as String? ?? '',
    status: j['status'] as String? ?? 'completed',
    duration: j['duration'] as String?,
  );
}

/// The session was compacted: a short summary of what was folded away.
class CompactionItem extends TimelineItem {
  const CompactionItem(this.text);
  final String text;
}

class UserItem extends TimelineItem {
  const UserItem(this.text);
  final String text;
}

class AssistantItem extends TimelineItem {
  const AssistantItem(this.text);
  final String text;
}

class ThinkingItem extends TimelineItem {
  const ThinkingItem(this.text);
  final String text;
}

/// A subagent a `task` call started.
class SpawnedAgent {
  const SpawnedAgent({required this.name, this.agent, required this.task});
  final String name;
  final String? agent;
  final String task;

  factory SpawnedAgent.fromJson(Map<String, dynamic> j) => SpawnedAgent(
    name: j['name'] as String? ?? '',
    agent: j['agent'] as String?,
    task: j['task'] as String? ?? '',
  );
}

/// A tool call. [status]: `running` | `ok` | `error`.
class ToolItem extends TimelineItem {
  const ToolItem({
    required this.id,
    required this.name,
    this.intent,
    this.detail,
    required this.status,
    this.output,
    this.agents = const [],
  });
  final String id;
  final String name;
  final String? intent;
  final String? detail;
  final String status;
  final String? output;

  /// Subagents started by this call (omp `task` only).
  final List<SpawnedAgent> agents;

  bool get isTask => agents.isNotEmpty;

  /// What the row says: the agent's own words for the step, else the raw command/path.
  String get summary {
    final i = intent?.trim();
    if (i != null && i.isNotEmpty) return i;
    final d = detail?.trim();
    return d != null && d.isNotEmpty ? d : name;
  }

  factory ToolItem.fromJson(Map<String, dynamic> j) => ToolItem(
    id: j['id'] as String? ?? '',
    name: j['name'] as String? ?? 'tool',
    intent: j['intent'] as String?,
    detail: j['detail'] as String?,
    status: j['status'] as String? ?? 'ok',
    output: j['output'] as String?,
    agents: [
      for (final a in (j['agents'] as List?) ?? const [])
        SpawnedAgent.fromJson((a as Map).cast<String, dynamic>()),
    ],
  );
}
