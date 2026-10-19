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

/// Scope of a PTY (which agent pane / worktree / tab it belongs to).
class PtyScope {
  final String? worktreeId;
  final String? tabId;
  final String? paneKey;

  const PtyScope({this.worktreeId, this.tabId, this.paneKey});

  factory PtyScope.fromJson(Map<String, dynamic> j) => PtyScope(
    worktreeId: j['worktree_id'] as String?,
    tabId: j['tab_id'] as String?,
    paneKey: j['pane_key'] as String?,
  );
}

/// One entry from `pty_list`.
class PtySummary {
  final String pty;
  final String cwd;
  final List<String> argv;
  final PtyScope scope;
  final String? title;
  final int? exited;

  const PtySummary({
    required this.pty,
    required this.cwd,
    required this.argv,
    required this.scope,
    this.title,
    this.exited,
  });

  factory PtySummary.fromJson(Map<String, dynamic> j) => PtySummary(
    pty: j['pty'] as String? ?? '',
    cwd: j['cwd'] as String? ?? '',
    argv: (j['argv'] as List<dynamic>?)?.cast<String>() ?? [],
    scope: j['scope'] is Map<String, dynamic>
        ? PtyScope.fromJson(j['scope'] as Map<String, dynamic>)
        : const PtyScope(),
    title: j['title'] as String?,
    exited: j['exited'] as int?,
  );
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
