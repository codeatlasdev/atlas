/// Atlas Crew protocol models (hostd `crewmate_*` / `crew_*` payloads): crewmate list
/// entries and tasks, automation triggers, the task action card, the crew-server GitHub
/// login and the pairing invite pushed by a crew server. Plain Dart, no codegen.
library;

/// The kind discriminant of a crewmate automation trigger.
/// JSON: `{"type": "schedule", "cron": "…"}` etc. (serde tag = "type").
sealed class TriggerKind {
  const TriggerKind();

  factory TriggerKind.fromJson(Map<String, dynamic> j) {
    final type = j['type'] as String? ?? '';
    return switch (type) {
      'schedule' => ScheduleTrigger(cron: j['cron'] as String? ?? ''),
      'interval' => IntervalTrigger(
        everySecs: (j['every_secs'] as num?)?.toInt() ?? 60,
      ),
      'webhook' => WebhookTrigger(token: j['token'] as String? ?? ''),
      'github' => GithubTrigger(
        repo: j['repo'] as String? ?? '',
        events: (j['events'] as List?)?.cast<String>() ?? const [],
      ),
      'file' => FileTrigger(
        path: j['path'] as String? ?? '',
        glob: j['glob'] as String?,
      ),
      'crewmate' => CrewmateTrigger(
        fromCrewmate: j['from_crewmate'] as String? ?? '',
        on: j['on'] as String? ?? 'done',
      ),
      'service' => ServiceTrigger(
        projectDir: j['project_dir'] as String? ?? '',
        service: j['service'] as String? ?? '',
        on: j['on'] as String? ?? 'failed',
      ),
      _ => ScheduleTrigger(cron: ''),
    };
  }

  Map<String, dynamic> toJson();
}

class ScheduleTrigger extends TriggerKind {
  const ScheduleTrigger({required this.cron});
  final String cron;
  @override
  Map<String, dynamic> toJson() => {'type': 'schedule', 'cron': cron};
}

class IntervalTrigger extends TriggerKind {
  const IntervalTrigger({required this.everySecs});
  final int everySecs;
  @override
  Map<String, dynamic> toJson() => {
    'type': 'interval',
    'every_secs': everySecs,
  };
}

class WebhookTrigger extends TriggerKind {
  const WebhookTrigger({required this.token});
  final String token;
  @override
  Map<String, dynamic> toJson() => {'type': 'webhook', 'token': token};
}

class GithubTrigger extends TriggerKind {
  const GithubTrigger({required this.repo, required this.events});
  final String repo;
  final List<String> events;
  @override
  Map<String, dynamic> toJson() => {
    'type': 'github',
    'repo': repo,
    'events': events,
  };
}

class FileTrigger extends TriggerKind {
  const FileTrigger({required this.path, this.glob});
  final String path;
  final String? glob;
  @override
  Map<String, dynamic> toJson() => {
    'type': 'file',
    'path': path,
    if (glob != null) 'glob': glob,
  };
}

class CrewmateTrigger extends TriggerKind {
  const CrewmateTrigger({required this.fromCrewmate, required this.on});
  final String fromCrewmate;
  final String on; // 'done' | 'failed'
  @override
  Map<String, dynamic> toJson() => {
    'type': 'crewmate',
    'from_crewmate': fromCrewmate,
    'on': on,
  };
}

class ServiceTrigger extends TriggerKind {
  const ServiceTrigger({
    required this.projectDir,
    required this.service,
    required this.on,
  });
  final String projectDir;
  final String service;
  final String on; // 'failed' | 'recovered'
  @override
  Map<String, dynamic> toJson() => {
    'type': 'service',
    'project_dir': projectDir,
    'service': service,
    'on': on,
  };
}

/// A crewmate automation trigger (schedule, interval, webhook, github, file, crewmate, service).
class Trigger {
  const Trigger({
    required this.id,
    required this.kind,
    required this.enabled,
    required this.prompt,
    this.lastFired,
    this.tz,
    this.nextFire,
  });
  final String id;
  final TriggerKind kind;
  final bool enabled;
  final String prompt;
  final int? lastFired;
  final String? tz;

  /// Unix seconds of the next scheduled fire; computed by hostd on crewmate_list.
  final int? nextFire;

  factory Trigger.fromJson(Map<String, dynamic> j) => Trigger(
    id: j['id'] as String? ?? '',
    kind: TriggerKind.fromJson((j['kind'] as Map).cast<String, dynamic>()),
    enabled: j['enabled'] as bool? ?? true,
    prompt: j['prompt'] as String? ?? '',
    lastFired: (j['last_fired'] as num?)?.toInt(),
    tz: j['tz'] as String?,
    nextFire: (j['next_fire'] as num?)?.toInt(),
  );

  Map<String, dynamic> toJson() => {
    'id': id,
    'kind': kind.toJson(),
    'enabled': enabled,
    'prompt': prompt,
    if (lastFired != null) 'last_fired': lastFired,
    if (tz != null) 'tz': tz,
  };

  Trigger copyWith({
    bool? enabled,
    String? prompt,
    TriggerKind? kind,
    String? tz,
  }) => Trigger(
    id: id,
    kind: kind ?? this.kind,
    enabled: enabled ?? this.enabled,
    prompt: prompt ?? this.prompt,
    lastFired: lastFired,
    tz: tz ?? this.tz,
    nextFire: nextFire,
  );
}

/// A crew action that blocks a task, sent when state is `waiting_action`.
class TaskAction {
  const TaskAction({required this.kind, required this.data});
  final String kind;
  final Map<String, dynamic> data;

  String? get reason => data['reason'] as String?;
  String? get name => data['name'] as String?;
  String? get label => data['label'] as String?;
  String? get placeholder => data['placeholder'] as String?;
  String? get question => data['question'] as String?;
  List<String> get options =>
      (data['options'] as List?)?.cast<String>() ?? const [];
  String? get engine => data['engine'] as String?;

  /// For `add_trigger` actions: the proposed trigger kind.
  TriggerKind? get triggerKind {
    if (kind != 'add_trigger') return null;
    final t = data['trigger'];
    if (t == null) return null;
    return TriggerKind.fromJson((t as Map).cast<String, dynamic>());
  }

  /// For `add_trigger` actions: the proposed prompt text.
  String? get triggerPrompt =>
      kind == 'add_trigger' ? data['prompt'] as String? : null;

  factory TaskAction.fromJson(Map<String, dynamic> j) =>
      TaskAction(kind: j['kind'] as String? ?? '', data: j);
}

/// A task enqueued or completed on a crewmate.
class CrewmateTask {
  final String id;
  final int ts;
  final String source;
  final String prompt;
  final String state; // queued|running|done|failed|cancelled|waiting_action
  final String? pane;
  final String? result;
  final String? error;
  final int? startedAt;
  final int? finishedAt;
  final List<CrewmateStep> steps;
  final TaskAction? action;
  final String? replyFull;
  final bool summarized;

  /// The agent's live title for the work (omp session title).
  final String? title;

  /// Repo being downloaded before the task can start (e.g. "codeatlasdev/ui").
  final String? preparing;

  /// IANA timezone stamped when the task was created.
  final String? tz;

  const CrewmateTask({
    required this.id,
    required this.ts,
    required this.source,
    required this.prompt,
    required this.state,
    this.pane,
    this.result,
    this.error,
    this.startedAt,
    this.finishedAt,
    this.steps = const [],
    this.action,
    this.replyFull,
    this.summarized = false,
    this.title,
    this.preparing,
    this.tz,
  });

  factory CrewmateTask.fromJson(Map<String, dynamic> j) => CrewmateTask(
    id: j['id'] as String? ?? '',
    ts: (j['ts'] as num?)?.toInt() ?? 0,
    source: j['source'] as String? ?? '',
    prompt: j['prompt'] as String? ?? '',
    state: j['state'] as String? ?? 'queued',
    pane: j['pane'] as String?,
    result: j['result'] as String?,
    error: j['error'] as String?,
    startedAt: (j['started_at'] as num?)?.toInt(),
    finishedAt: (j['finished_at'] as num?)?.toInt(),
    steps: [
      for (final s in (j['steps'] as List<dynamic>?) ?? const [])
        CrewmateStep.fromJson((s as Map).cast<String, dynamic>()),
    ],
    action: j['action'] == null
        ? null
        : TaskAction.fromJson((j['action'] as Map).cast<String, dynamic>()),
    replyFull: j['reply_full'] as String?,
    summarized: j['summarized'] as bool? ?? false,
    title: j['title'] as String?,
    preparing: j['preparing'] as String?,
    tz: j['tz'] as String?,
  );
}

/// Pending approval on a crewmate (from crewmate_list pending_approval field).
class CrewmatePendingApproval {
  final String req;
  final String? toolName;
  final dynamic toolInput;
  final String paneKey;

  const CrewmatePendingApproval({
    required this.req,
    this.toolName,
    this.toolInput,
    required this.paneKey,
  });

  factory CrewmatePendingApproval.fromJson(Map<String, dynamic> j) =>
      CrewmatePendingApproval(
        req: j['req'] as String? ?? '',
        toolName: j['tool_name'] as String?,
        toolInput: j['tool_input'],
        paneKey: j['pane_key'] as String? ?? '',
      );
}

class CrewmateStep {
  const CrewmateStep({required this.ts, required this.text, this.group});
  final int ts;
  final String text;

  /// Agent title grouping this step ("CART: Testar store api e carrinho").
  final String? group;
  factory CrewmateStep.fromJson(Map<String, dynamic> j) => CrewmateStep(
    ts: (j['ts'] as num?)?.toInt() ?? 0,
    text: j['text'] as String? ?? '',
    group: j['group'] as String?,
  );
}

class CrewmateLast {
  const CrewmateLast({
    required this.text,
    required this.state,
    required this.ts,
  });
  final String text;
  final String state;
  final int ts;
  factory CrewmateLast.fromJson(Map<String, dynamic> j) => CrewmateLast(
    text: j['text'] as String? ?? '',
    state: j['state'] as String? ?? '',
    ts: (j['ts'] as num?)?.toInt() ?? 0,
  );
}

/// Running task reference from crewmate_list: {id, pane} object.
/// The real producer (crew/mod.rs crewmate_list) emits this as an object,
/// never a bare string.
class CrewmateRunningTask {
  const CrewmateRunningTask({required this.id, required this.pane});
  final String id;
  final String pane;
  factory CrewmateRunningTask.fromJson(Map<String, dynamic> j) =>
      CrewmateRunningTask(
        id: j['id'] as String? ?? '',
        pane: j['pane'] as String? ?? '',
      );
}

class CrewmateEntry {
  const CrewmateEntry({
    required this.id,
    required this.name,
    required this.role,
    required this.agent,
    required this.approval,
    required this.state,
    this.approvalSupported = true,
    this.runningTask,
    this.queued = 0,
    this.last,
    this.pendingApproval,
    this.triggers = const [],
    this.shortReplies,
    this.look,
  });
  final String id;
  final String name;
  final String role;

  /// The crewmate's look as the crew server stores it (`look` in crewmate_list), untrusted:
  /// `CrewLook.from` keeps each valid field and falls back per field.
  final Map<String, dynamic>? look;
  final String agent;
  final String approval; // ask | auto
  final String state; // idle | working | needs_you
  final bool approvalSupported;
  final CrewmateRunningTask? runningTask;
  final int queued;
  final CrewmateLast? last;
  final CrewmatePendingApproval? pendingApproval;
  final List<Trigger> triggers;
  final bool? shortReplies;
  factory CrewmateEntry.fromJson(Map<String, dynamic> j) => CrewmateEntry(
    id: j['id'] as String? ?? '',
    name: j['name'] as String? ?? '',
    role: j['role'] as String? ?? '',
    agent: j['agent'] as String? ?? '',
    approval: j['approval'] as String? ?? 'ask',
    state: j['state'] as String? ?? 'idle',
    approvalSupported: j['approval_supported'] as bool? ?? true,
    runningTask: j['running_task'] == null
        ? null
        : CrewmateRunningTask.fromJson((j['running_task'] as Map).cast()),
    queued: (j['queued'] as num?)?.toInt() ?? 0,
    last: j['last'] == null
        ? null
        : CrewmateLast.fromJson((j['last'] as Map).cast()),
    pendingApproval: j['pending_approval'] == null
        ? null
        : CrewmatePendingApproval.fromJson(
            (j['pending_approval'] as Map).cast(),
          ),
    triggers: [
      for (final t in (j['triggers'] as List<dynamic>?) ?? const [])
        Trigger.fromJson((t as Map).cast()),
    ],
    shortReplies: j['short_replies'] as bool?,
    look: (j['look'] as Map?)?.cast<String, dynamic>(),
  );
}

class ComputerInvite {
  const ComputerInvite({required this.url, required this.name});
  final String url;
  final String name;
}

/// GitHub status on a crew server (`crew_github_status`).
class CrewGithubStatus {
  const CrewGithubStatus({required this.installed, required this.login});

  final bool installed;
  final String? login;

  factory CrewGithubStatus.fromJson(Map<String, dynamic> j) => CrewGithubStatus(
    installed: j['installed'] as bool? ?? false,
    login: j['login'] as String?,
  );
}

/// Active device-flow login for a crew server (`crew_github_login_start`).
class CrewGithubLogin {
  const CrewGithubLogin({
    required this.userCode,
    required this.verificationUri,
  });

  final String userCode;
  final String verificationUri;

  factory CrewGithubLogin.fromJson(Map<String, dynamic> j) => CrewGithubLogin(
    userCode: j['user_code'] as String? ?? '',
    verificationUri: j['verification_uri'] as String? ?? '',
  );
}
