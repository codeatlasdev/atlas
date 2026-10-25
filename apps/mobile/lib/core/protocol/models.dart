/// Atlas Mobile protocol models — plain Dart, no codegen.
library;

/// An agent installed on the computer and whether its hooks report to Atlas
/// (hostd `agent_integrations`). `state`: `ok` | `missing` | `unknown`.
class AgentIntegration {
  const AgentIntegration({
    required this.id,
    required this.label,
    required this.state,
  });
  final String id;
  final String label;
  final String state;

  factory AgentIntegration.fromJson(Map<String, dynamic> j) => AgentIntegration(
    id: j['id'] as String,
    label: j['label'] as String? ?? j['id'] as String,
    state: j['state'] as String? ?? 'unknown',
  );
}

/// Agent state as pushed by hostd `agent_status` events.
class AgentStatus {
  final String state;
  final String agent;
  final String? sessionId;
  final String? sessionFile;
  final String? model;
  final String? prompt;
  final String? lastMessage;
  final int updatedAt;
  final int stateStartedAt;
  final String? worktreeId;
  final String? tabId;

  /// The session's title (omp names it; null until it has one).
  final String? title;

  const AgentStatus({
    required this.state,
    required this.agent,
    this.sessionId,
    this.sessionFile,
    this.model,
    this.prompt,
    this.lastMessage,
    required this.updatedAt,
    required this.stateStartedAt,
    this.worktreeId,
    this.tabId,
    this.title,
  });

  factory AgentStatus.fromJson(Map<String, dynamic> j) => AgentStatus(
    state: j['state'] as String? ?? 'idle',
    agent: j['agent'] as String? ?? '',
    sessionId: j['session_id'] as String?,
    sessionFile: j['session_file'] as String?,
    model: j['model'] as String?,
    prompt: j['prompt'] as String?,
    lastMessage: j['last_message'] as String?,
    updatedAt: (j['updated_at'] as num?)?.toInt() ?? 0,
    stateStartedAt: (j['state_started_at'] as num?)?.toInt() ?? 0,
    worktreeId: j['worktree_id'] as String?,
    tabId: j['tab_id'] as String?,
    title: j['title'] as String?,
  );

  @override
  bool operator ==(Object other) =>
      other is AgentStatus &&
      other.state == state &&
      other.updatedAt == updatedAt &&
      other.title == title;

  @override
  int get hashCode => Object.hash(state, updatedAt, title);
}

/// One service entry from `project_list`.
class ServiceInfo {
  final String name;
  final String state;
  final int? port;
  final String? health;

  const ServiceInfo({
    required this.name,
    required this.state,
    this.port,
    this.health,
  });

  factory ServiceInfo.fromJson(Map<String, dynamic> j) => ServiceInfo(
    name: j['name'] as String? ?? '',
    state: j['state'] as String? ?? 'stopped',
    port: j['port'] as int?,
    health: j['health'] as String?,
  );
}

/// One project entry from `project_list`.
class ProjectInfo {
  final String id;
  final String root;
  final String name;
  final bool running;
  final List<ServiceInfo> services;

  /// Agents whose terminal runs in this project: `(pane key, status)`.
  final List<(String, AgentStatus)> agents;

  const ProjectInfo({
    required this.id,
    required this.root,
    required this.name,
    required this.running,
    required this.services,
    this.agents = const [],
  });

  factory ProjectInfo.fromJson(Map<String, dynamic> j) => ProjectInfo(
    id: j['id'] as String? ?? '',
    root: j['root'] as String? ?? '',
    name: j['name'] as String? ?? '',
    running: j['running'] as bool? ?? false,
    services:
        (j['services'] as List<dynamic>?)
            ?.map((e) => ServiceInfo.fromJson(e as Map<String, dynamic>))
            .toList() ??
        [],
    agents: [
      for (final a in (j['agents'] as List<dynamic>?) ?? const [])
        (
          (a as Map)['pane_key'] as String,
          AgentStatus.fromJson((a['status'] as Map).cast<String, dynamic>()),
        ),
    ],
  );
}

/// A paired Mac device stored in secure storage.
class MacDevice {
  final String nodeId;
  final String name;
  final List<String> addrs;
  final String? relay;

  const MacDevice({
    required this.nodeId,
    required this.name,
    this.addrs = const [],
    this.relay,
  });

  factory MacDevice.fromJson(Map<String, dynamic> j) => MacDevice(
    nodeId: j['nodeId'] as String? ?? j['node_id'] as String? ?? '',
    name: j['name'] as String? ?? '',
    addrs: (j['addrs'] as List<dynamic>?)?.cast<String>() ?? [],
    relay: j['relay'] as String?,
  );

  Map<String, dynamic> toJson() => {
    'nodeId': nodeId,
    'name': name,
    'addrs': addrs,
    if (relay != null) 'relay': relay,
  };

  @override
  bool operator ==(Object other) =>
      other is MacDevice && other.nodeId == nodeId;

  @override
  int get hashCode => nodeId.hashCode;
}

/// Pending agent approval (from `agent_approval` push event).
class AgentApproval {
  final String paneKey;
  final String req;
  final String? toolName;
  final dynamic toolInput;
  final String? agent;

  const AgentApproval({
    required this.paneKey,
    required this.req,
    this.toolName,
    this.toolInput,
    this.agent,
  });

  factory AgentApproval.fromJson(Map<String, dynamic> j) => AgentApproval(
    paneKey: j['pane_key'] as String? ?? '',
    req: j['req'] as String? ?? '',
    toolName: j['tool_name'] as String?,
    toolInput: j['tool_input'],
    agent: j['agent'] as String?,
  );
}

/// Information parsed from an `atlas-mobile://pair?...` QR URL.
class PairInfo {
  final String nodeId;
  final String code;
  final String name;
  final List<String> addrs;
  final String? relay;

  const PairInfo({
    required this.nodeId,
    required this.code,
    required this.name,
    required this.addrs,
    this.relay,
  });

  /// Parses `atlas-mobile://pair?node=…&code=…&name=…[&addr=…]*[&relay=…]`.
  static PairInfo parse(String url) {
    final uri = Uri.parse(url);
    if (uri.scheme != 'atlas-mobile') {
      throw ArgumentError('Expected atlas-mobile:// scheme, got ${uri.scheme}');
    }
    if (uri.host != 'pair') {
      throw ArgumentError('Expected host "pair", got "${uri.host}"');
    }
    final node = uri.queryParameters['node'];
    final code = uri.queryParameters['code'];
    if (node == null || node.isEmpty) {
      throw ArgumentError('Missing required query parameter "node"');
    }
    if (code == null || code.isEmpty) {
      throw ArgumentError('Missing required query parameter "code"');
    }
    return PairInfo(
      nodeId: node,
      code: code,
      name: uri.queryParameters['name'] ?? '',
      addrs: uri.queryParametersAll['addr'] ?? [],
      relay: uri.queryParameters['relay'],
    );
  }
}

/// Returns true when the agent state allows sending chat messages.
/// Only `idle` and `done` states allow sending.
bool canSendChat(AgentStatus? status) {
  if (status == null) return false;
  return status.state == 'idle' || status.state == 'done';
}

/// One entry of an agent's conversation, as `agent_history` returns it.
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
    _ => null,
  };
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

/// A subagent's live state in the parent's `agent_history.subagents`.
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

/// What the timeline renders: user/assistant/thinking items as they are, and each run of
/// consecutive plain tool calls folded into one [ToolGroup]; `task` calls stay apart as
/// [SubagentsBlock]s.
sealed class TimelineBlock {
  const TimelineBlock();
}

class ItemBlock extends TimelineBlock {
  const ItemBlock(this.item);
  final TimelineItem item;
}

class ToolGroup extends TimelineBlock {
  const ToolGroup(this.calls);
  final List<ToolItem> calls;

  bool get running => calls.any((c) => c.status == 'running');
  int get errors => calls.where((c) => c.status == 'error').length;

  /// Distinct tool names in order of first use: `bash, read, edit`.
  List<String> get names => {for (final c in calls) c.name}.toList();
}

class SubagentsBlock extends TimelineBlock {
  const SubagentsBlock(this.call);
  final ToolItem call;
}

/// The summary card at the top of a finished turn.
class RecapBlock extends TimelineBlock {
  const RecapBlock(this.recap);
  final Recap recap;
}

/// Groups [items] for display. With [recapLast] the last turn also gets its recap
/// (the agent finished it); earlier turns always do, as they are over.
List<TimelineBlock> groupTimeline(
  List<TimelineItem> items, {
  bool recapLast = false,
  Duration? lastDuration,
}) {
  final out = <TimelineBlock>[];
  final run = <ToolItem>[];
  void flush() {
    if (run.isEmpty) return;
    out.add(ToolGroup(List.of(run)));
    run.clear();
  }

  final turns = splitTurns(items);
  // A turn's tool run never continues into the next turn: its user item flushes it.
  for (var t = 0; t < turns.length; t++) {
    final turn = turns[t];
    final last = t == turns.length - 1;
    final recap = last && !recapLast
        ? null
        : Recap.of(turn, duration: last ? lastDuration : null);
    for (var k = 0; k < turn.length; k++) {
      final i = turn[k];
      if (i is ToolItem && !i.isTask) {
        run.add(i);
        continue;
      }
      flush();
      out.add(i is ToolItem ? SubagentsBlock(i) : ItemBlock(i));
      // The recap sits right under the prompt that started the turn.
      if (k == 0 && i is UserItem && recap != null && !recap.isEmpty) {
        out.add(RecapBlock(recap));
      }
    }
  }
  flush();
  return out;
}

/// [items] cut into turns: each starts at a [UserItem] (the first may start without).
List<List<TimelineItem>> splitTurns(List<TimelineItem> items) {
  final turns = <List<TimelineItem>>[];
  for (final i in items) {
    if (i is UserItem || turns.isEmpty) turns.add([]);
    turns.last.add(i);
  }
  return turns;
}

/// What one turn did, computed from its items alone.
class Recap {
  const Recap({
    this.files = const [],
    this.commands = 0,
    this.finished = 0,
    this.failed = 0,
    this.duration,
  });

  /// Distinct paths touched by `edit`/`write`/`ast_edit`, in order.
  final List<String> files;

  /// `bash` calls.
  final int commands;

  /// Subagents that finished well / badly (`result` items).
  final int finished;
  final int failed;
  final Duration? duration;

  bool get isEmpty =>
      files.isEmpty && commands == 0 && finished == 0 && failed == 0;

  static const _writers = {'edit', 'write', 'ast_edit'};

  factory Recap.of(List<TimelineItem> turn, {Duration? duration}) {
    final files = <String>{};
    var commands = 0, finished = 0, failed = 0;
    for (final i in turn) {
      if (i is ToolItem) {
        if (_writers.contains(i.name) && i.status != 'error') {
          final p = i.detail?.trim();
          if (p != null && p.isNotEmpty) files.add(p);
        } else if (i.name == 'bash') {
          commands++;
        }
      } else if (i is ResultItem) {
        i.status == 'completed' ? finished++ : failed++;
      }
    }
    return Recap(
      files: files.toList(),
      commands: commands,
      finished: finished,
      failed: failed,
      duration: duration,
    );
  }
}

/// One node of `agent_tree`: the root agent (`id == ''`) or a subagent.
/// [status]: `pending` | `running` | `completed` | `failed` | `aborted` | `idle`.
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

  bool get isRoot => parent == null;
  bool get running => status == 'running';
  bool get failed => status == 'failed' || status == 'aborted';
  bool get done => status == 'completed';

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
