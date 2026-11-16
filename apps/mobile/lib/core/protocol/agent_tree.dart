/// `agent_tree`: the agent, its subagents and their live figures.
library;

import 'package:atlas_mobile/core/protocol/todo_list.dart';

/// One tool call of a node's own recent activity: [at] epoch ms (0 = unknown), [status]
/// `running` | `ok` | `error`.
class RecentCall {
  const RecentCall({
    required this.at,
    required this.tool,
    required this.intent,
    required this.status,
  });
  final int at;
  final String tool;
  final String intent;
  final String status;

  factory RecentCall.fromJson(Map<String, dynamic> j) => RecentCall(
    at: (j['at'] as num?)?.toInt() ?? 0,
    tool: j['tool'] as String? ?? 'tool',
    intent: j['intent'] as String? ?? '',
    status: j['status'] as String? ?? 'ok',
  );
}

/// One node of `agent_tree`: the root agent (`id == ''`) or a subagent.
/// [status]: `pending` | `running` | `completed` | `failed` | `aborted` | `idle`.
/// The hub's figures ([agentType] … [recent]) are absent from an older hostd.
class AgentNode {
  const AgentNode({
    required this.id,
    this.parent,
    this.name,
    this.agent,
    this.model,
    required this.status,
    this.task,
    this.durationMs = 0,
    this.tokens = 0,
    this.cost = 0,
    this.toolCount = 0,
    this.now,
    this.report,
    this.updatedAt = 0,
    this.agentType,
    this.thinking,
    this.requests = 0,
    this.activeMs = 0,
    this.contextTokens,
    this.contextWindow,
    this.recent = const [],
    this.todos,
  });
  final String id;
  final String? parent;
  final String? name;
  final String? agent;
  final String? model;
  final String status;
  final String? task;
  final int durationMs;
  final int tokens;
  final double cost;
  final int toolCount;

  /// What the node is doing this moment (its running tool), if anything.
  final String? now;
  final String? report;
  final int updatedAt;

  /// omp's agent definition (`task`, `scout`); null for the root and for other agents.
  final String? agentType;

  /// The thinking level (`high`, `off`, …).
  final String? thinking;
  final int requests;

  /// Time spent working, parked gaps left out.
  final int activeMs;
  final int? contextTokens;
  final int? contextWindow;

  /// Its last tool calls, oldest first.
  final List<RecentCall> recent;

  /// The root's todo list; null when it has none.
  final TodoList? todos;

  bool get isRoot => parent == null;
  bool get running => status == 'running';
  bool get failed => status == 'failed' || status == 'aborted';
  bool get done => status == 'completed';

  /// Share of the context window in use, 0..1; null when either figure is unknown.
  double? get contextShare {
    final t = contextTokens, w = contextWindow;
    if (t == null || w == null || w <= 0) return null;
    return (t / w).clamp(0.0, 1.0);
  }

  /// What to call the node: its `name` (`Deep`), else the last dotted segment of its
  /// id; the session title for the root.
  String get label {
    final n = name?.trim();
    if (isRoot) return n ?? '';
    return n != null && n.isNotEmpty ? n.split('.').last : id.split('.').last;
  }

  factory AgentNode.fromJson(Map<String, dynamic> j) => AgentNode(
    id: j['id'] as String? ?? '',
    parent: j['parent'] as String?,
    name: j['name'] as String?,
    agent: j['agent'] as String?,
    model: j['model'] as String?,
    status: j['status'] as String? ?? 'pending',
    task: j['task'] as String?,
    durationMs: (j['duration_ms'] as num?)?.toInt() ?? 0,
    tokens: (j['tokens'] as num?)?.toInt() ?? 0,
    cost: (j['cost'] as num?)?.toDouble() ?? 0,
    toolCount: (j['tool_count'] as num?)?.toInt() ?? 0,
    now: j['now'] as String?,
    report: j['report'] as String?,
    updatedAt: (j['updated_at'] as num?)?.toInt() ?? 0,
    agentType: j['agent_type'] as String?,
    thinking: j['thinking'] as String?,
    requests: (j['requests'] as num?)?.toInt() ?? 0,
    activeMs: (j['active_ms'] as num?)?.toInt() ?? 0,
    contextTokens: (j['context_tokens'] as num?)?.toInt(),
    contextWindow: (j['context_window'] as num?)?.toInt(),
    recent: [
      for (final r in (j['recent'] as List?) ?? const [])
        RecentCall.fromJson((r as Map).cast<String, dynamic>()),
    ],
    todos: TodoList.fromJson(j['todos']),
  );
}

/// `agent_tree`: the agent and its nested subagents.
class AgentTree {
  const AgentTree(this.nodes);
  static const empty = AgentTree([]);
  final List<AgentNode> nodes;

  factory AgentTree.fromJson(Map<String, dynamic> j) => AgentTree([
    for (final n in (j['nodes'] as List?) ?? const [])
      AgentNode.fromJson((n as Map).cast<String, dynamic>()),
  ]);

  AgentNode? node(String id) {
    for (final n in nodes) {
      if (n.id == id) return n;
    }
    return null;
  }

  List<AgentNode> children(String id) => [
    for (final n in nodes)
      if (!n.isRoot && n.parent == id) n,
  ];

  /// Nodes in depth-first order, each with its depth (root 0); orphans (a parent the
  /// tree does not list) hang off the root.
  List<(AgentNode, int)> get flat {
    final out = <(AgentNode, int)>[];
    final known = {for (final n in nodes) n.id};
    void walk(AgentNode n, int depth) {
      out.add((n, depth));
      for (final c in children(n.id)) {
        walk(c, depth + 1);
      }
    }

    final root = node('');
    if (root != null) walk(root, 0);
    for (final n in nodes) {
      if (!n.isRoot &&
          !known.contains(n.parent) &&
          !out.any((e) => e.$1 == n)) {
        walk(n, 1);
      }
    }
    return out;
  }

  /// The chain from the root to [id]: `[root, Scout, Scout.Deep]`.
  List<AgentNode> path(String id) {
    final out = <AgentNode>[];
    var cur = node(id);
    while (cur != null && out.length <= nodes.length) {
      out.insert(0, cur);
      final p = cur.parent;
      cur = p == null ? null : node(p);
    }
    return out;
  }

  /// Subagents (not the root).
  Iterable<AgentNode> get subs => nodes.where((n) => !n.isRoot);
  int get done => subs.where((n) => n.done).length;
  int get running => subs.where((n) => n.running).length;
  int get failed => subs.where((n) => n.failed).length;
  bool get anyRunning => nodes.any((n) => n.running);

  /// `3/5 done · 2 running`; empty when there are no subagents.
  String get summary {
    final n = subs.length;
    if (n == 0) return '';
    return [
      '$done/$n done',
      if (running > 0) '$running running',
      if (failed > 0) '$failed failed',
    ].join(' · ');
  }

  /// Siblings of [id] in tree order (same parent, itself included).
  List<AgentNode> siblings(String id) {
    final n = node(id);
    if (n == null) return const [];
    return n.isRoot ? [n] : children(n.parent!);
  }
}
