/// `agent_history`: a conversation as a timeline plus its subagents.
library;

import 'package:atlas_mobile/core/protocol/timeline_items.dart';

/// [state]: `running` | `done`.
class SubagentInfo {
  const SubagentInfo({
    required this.name,
    required this.state,
    required this.updatedAt,
    this.report,
  });
  final String name;
  final String state;
  final int updatedAt;
  final String? report;

  bool get running => state == 'running';

  factory SubagentInfo.fromJson(Map<String, dynamic> j) => SubagentInfo(
    name: j['name'] as String? ?? '',
    state: j['state'] as String? ?? 'done',
    updatedAt: (j['updated_at'] as num?)?.toInt() ?? 0,
    report: j['report'] as String?,
  );
}

/// `agent_history`: the conversation as a timeline, the session title and the
/// subagents (absent when reading a subagent's own transcript).
class AgentHistory {
  const AgentHistory({
    this.items = const [],
    this.title,
    this.truncated = false,
    this.subagents = const [],
  });
  final List<TimelineItem> items;
  final String? title;
  final bool truncated;
  final List<SubagentInfo> subagents;

  SubagentInfo? subagent(String name) {
    for (final s in subagents) {
      if (s.name == name) return s;
    }
    return null;
  }

  factory AgentHistory.fromJson(Map<String, dynamic> j) {
    final items = [
      for (final i in (j['items'] as List?) ?? const [])
        ?TimelineItem.fromJson((i as Map).cast<String, dynamic>()),
    ];
    return AgentHistory(
      // An older hostd has no `items`: fall back to its plain messages.
      items: items.isNotEmpty || j['items'] != null
          ? items
          : [
              for (final m in (j['messages'] as List?) ?? const [])
                if ((m as Map)['role'] == 'user')
                  UserItem(m['text'] as String? ?? '')
                else
                  AssistantItem(m['text'] as String? ?? ''),
            ],
      title: j['title'] as String?,
      truncated: j['truncated'] == true,
      subagents: [
        for (final s in (j['subagents'] as List?) ?? const [])
          SubagentInfo.fromJson((s as Map).cast<String, dynamic>()),
      ],
    );
  }
}
