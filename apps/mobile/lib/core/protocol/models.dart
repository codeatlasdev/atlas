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
    _ => null,
  };
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

List<TimelineBlock> groupTimeline(List<TimelineItem> items) {
  final out = <TimelineBlock>[];
  final run = <ToolItem>[];
  void flush() {
    if (run.isEmpty) return;
    out.add(ToolGroup(List.of(run)));
    run.clear();
  }

  for (final i in items) {
    if (i is ToolItem && !i.isTask) {
      run.add(i);
      continue;
    }
    flush();
    out.add(i is ToolItem ? SubagentsBlock(i) : ItemBlock(i));
  }
  flush();
  return out;
}
