/// Atlas Mobile protocol models of the computer: the agents it runs and their status,
/// projects, services and paired devices. Plain Dart, no codegen.
library;

/// An agent installed on the computer and whether its hooks report to Atlas
/// (hostd `agent_integrations`). `state`: `ok` | `missing` | `other_version`
/// (hooks on, written by another atlas) | `unknown`. Only `missing` is "off".
class AgentIntegration {
  const AgentIntegration({
    required this.id,
    required this.label,
    required this.setup,
    required this.state,
  });
  final String id;
  final String label;

  /// `atlas setup` name, e.g. `claude-code`.
  final String setup;
  final String state;

  factory AgentIntegration.fromJson(Map<String, dynamic> j) => AgentIntegration(
    id: j['id'] as String,
    label: j['label'] as String? ?? j['id'] as String,
    setup: j['setup'] as String? ?? j['id'] as String,
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

  /// `[{pane_key, status}]` → `(pane key, status)` pairs; absent means none.
  static List<(String, AgentStatus)> paneList(Object? raw) => [
    for (final a in (raw as List?) ?? const [])
      (
        (a as Map)['pane_key'] as String,
        AgentStatus.fromJson((a['status'] as Map).cast<String, dynamic>()),
      ),
  ];
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

  /// `false` for a folder that is neither an atlas.yaml project nor a git checkout (`~/Downloads` opened once in
  /// the desktop). An older hostd sends no `kind`: everything is a project, as before.
  final bool isProject;

  /// The main repository's root this checkout belongs to (its own root for a main checkout); null when not git
  /// or from an older hostd. Worktrees of one repository share it.
  final String? repo;

  /// The checked-out branch; null when detached or not git.
  final String? branch;

  /// A linked git worktree (a second checkout of [repo]).
  final bool worktree;

  /// Pinned in the desktop's sidebar.
  final bool pinned;

  const ProjectInfo({
    required this.id,
    required this.root,
    required this.name,
    required this.running,
    required this.services,
    this.agents = const [],
    this.isProject = true,
    this.repo,
    this.branch,
    this.worktree = false,
    this.pinned = false,
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
    agents: AgentStatus.paneList(j['agents']),
    isProject: j['kind'] != 'folder',
    repo: j['repo'] as String?,
    branch: j['branch'] as String?,
    worktree: j['worktree'] as bool? ?? false,
    pinned: j['pinned'] as bool? ?? false,
  );
}

/// A paired Mac device stored in secure storage.
class MacDevice {
  final String nodeId;
  final String name;
  final List<String> addrs;
  final String? relay;
  final List<String> caps;

  /// A crew server reached through the paired Mac [via] (its hostd relays to
  /// [crewServer], the desktop's ssh target). Never stored: listed by `crew_servers`.
  final MacDevice? via;
  final String? crewServer;

  const MacDevice({
    required this.nodeId,
    required this.name,
    this.addrs = const [],
    this.relay,
    this.caps = const [],
    this.via,
    this.crewServer,
  });

  /// The same computer with no direct addresses: only its relay and its id (discovery) remain to reach it.
  MacDevice withoutLanHints() => MacDevice(
    nodeId: nodeId,
    name: name,
    relay: relay,
    caps: caps,
    via: via,
    crewServer: crewServer,
  );

  /// Crew server [server] behind [mac]; its id is stable so providers keyed on it are
  /// too, and holds no `/`, so it fits one segment of the `/crew/:nodeId/:crewmateId` route.
  factory MacDevice.crew(MacDevice mac, String server, String name) =>
      MacDevice(
        nodeId: '${mac.nodeId}~crew~$server',
        name: name,
        via: mac,
        crewServer: server,
      );

  factory MacDevice.fromJson(Map<String, dynamic> j) => MacDevice(
    nodeId: j['nodeId'] as String? ?? j['node_id'] as String? ?? '',
    name: j['name'] as String? ?? '',
    addrs: (j['addrs'] as List<dynamic>?)?.cast<String>() ?? [],
    relay: j['relay'] as String?,
    caps: (j['caps'] as List<dynamic>?)?.cast<String>() ?? [],
  );

  Map<String, dynamic> toJson() => {
    'nodeId': nodeId,
    'name': name,
    'addrs': addrs,
    if (relay != null) 'relay': relay,
    if (caps.isNotEmpty) 'caps': caps,
  };

  @override
  bool operator ==(Object other) =>
      other is MacDevice && other.nodeId == nodeId;

  @override
  int get hashCode => nodeId.hashCode;
}

/// Returns true when the agent state allows sending chat messages.
/// Only `idle` and `done` states allow sending.
bool canSendChat(AgentStatus? status) {
  if (status == null) return false;
  return status.state == 'idle' || status.state == 'done';
}
