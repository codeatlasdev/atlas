/// Atlas Mobile protocol models — plain Dart, no codegen.
library;

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
  );

  @override
  bool operator ==(Object other) =>
      other is AgentStatus &&
      other.state == state &&
      other.updatedAt == updatedAt;

  @override
  int get hashCode => Object.hash(state, updatedAt);
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

/// A bot on a bot computer.
class Bot {
  final String id;
  final String name;
  final String role;
  final String agent;
  final String approval; // "ask" | "auto"
  final String state; // "idle" | "working" | "needs_you"
  final List<Routine> routines;
  final BotTask? runningTask;
  final BotPendingApproval? pendingApproval;

  const Bot({
    required this.id,
    required this.name,
    required this.role,
    required this.agent,
    required this.approval,
    required this.state,
    this.routines = const [],
    this.runningTask,
    this.pendingApproval,
  });

  factory Bot.fromJson(Map<String, dynamic> j) => Bot(
    id: j['id'] as String? ?? '',
    name: j['name'] as String? ?? '',
    role: j['role'] as String? ?? '',
    agent: j['agent'] as String? ?? '',
    approval: j['approval'] as String? ?? 'ask',
    state: j['state'] as String? ?? 'idle',
    routines: (j['routines'] as List<dynamic>?)
            ?.map((e) => Routine.fromJson(e as Map<String, dynamic>))
            .toList() ??
        const [],
    runningTask: j['running_task'] == null
        ? null
        : BotTask.fromJson((j['running_task'] as Map).cast<String, dynamic>()),
    pendingApproval: j['pending_approval'] == null
        ? null
        : BotPendingApproval.fromJson(
            (j['pending_approval'] as Map).cast<String, dynamic>()),
  );
}

/// A task enqueued or completed on a bot.
class BotTask {
  final String id;
  final int ts;
  final String source;
  final String prompt;
  final String state; // queued|running|done|failed|cancelled
  final String? pane;
  final String? result;
  final String? error;
  final int? finishedAt;

  const BotTask({
    required this.id,
    required this.ts,
    required this.source,
    required this.prompt,
    required this.state,
    this.pane,
    this.result,
    this.error,
    this.finishedAt,
  });

  factory BotTask.fromJson(Map<String, dynamic> j) => BotTask(
    id: j['id'] as String? ?? '',
    ts: (j['ts'] as num?)?.toInt() ?? 0,
    source: j['source'] as String? ?? '',
    prompt: j['prompt'] as String? ?? '',
    state: j['state'] as String? ?? 'queued',
    pane: j['pane'] as String?,
    result: j['result'] as String?,
    error: j['error'] as String?,
    finishedAt: (j['finished_at'] as num?)?.toInt(),
  );
}

/// A cron routine for a bot.
class Routine {
  final String id;
  final String cron;
  final String prompt;
  final bool enabled;
  final int? lastRun;

  const Routine({
    required this.id,
    required this.cron,
    required this.prompt,
    required this.enabled,
    this.lastRun,
  });

  factory Routine.fromJson(Map<String, dynamic> j) => Routine(
    id: j['id'] as String? ?? '',
    cron: j['cron'] as String? ?? '',
    prompt: j['prompt'] as String? ?? '',
    enabled: j['enabled'] as bool? ?? true,
    lastRun: (j['last_run'] as num?)?.toInt(),
  );

  Map<String, dynamic> toJson() => {
    'id': id,
    'cron': cron,
    'prompt': prompt,
    'enabled': enabled,
    if (lastRun != null) 'last_run': lastRun,
  };
}

/// Pending approval on a bot (from BotList pending_approval field).
class BotPendingApproval {
  final String req;
  final String? toolName;
  final dynamic toolInput;
  final String paneKey;

  const BotPendingApproval({
    required this.req,
    this.toolName,
    this.toolInput,
    required this.paneKey,
  });

  factory BotPendingApproval.fromJson(Map<String, dynamic> j) =>
      BotPendingApproval(
        req: j['req'] as String? ?? '',
        toolName: j['tool_name'] as String?,
        toolInput: j['tool_input'],
        paneKey: j['pane_key'] as String? ?? '',
      );
}
