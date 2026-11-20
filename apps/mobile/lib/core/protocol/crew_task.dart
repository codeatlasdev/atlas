/// A crewmate task and what hangs off it: the blocking action, attachments, GitHub origin.
library;

import 'package:atlas_mobile/core/protocol/ask_request.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/crew_triggers.dart';

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

  /// For `refused` actions: why Atlas's guards refused the work.
  List<String> get reasons =>
      (data['reasons'] as List?)?.cast<String>() ?? const [];

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

/// An image kept with a task (hostd `Attachment`). Never carries the bytes.
class TaskAttachment {
  final String name;
  final String mime;
  final int bytesLen;
  final String sha256;

  /// Where the agent finds it, relative to its work dir.
  final String path;

  /// `agent` for a capture the crewmate took; empty for an image the owner sent.
  final String source;

  const TaskAttachment({
    required this.name,
    required this.mime,
    required this.bytesLen,
    required this.sha256,
    required this.path,
    this.source = '',
  });

  /// A capture the crewmate took (what the cards show), not an image the owner sent.
  bool get fromAgent => source == 'agent';

  factory TaskAttachment.fromJson(Map<String, dynamic> j) => TaskAttachment(
    name: j['name'] as String? ?? '',
    mime: j['mime'] as String? ?? '',
    bytesLen: (j['bytes_len'] as num?)?.toInt() ?? 0,
    sha256: j['sha256'] as String? ?? '',
    path: j['path'] as String? ?? '',
    source: j['source'] as String? ?? '',
  );
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

  /// Why a queued task is not starting yet ("server busy": every agent slot of the crew server is
  /// taken); gone once it starts.
  final String? waitingFor;

  /// IANA timezone stamped when the task was created.
  final String? tz;

  /// The GitHub issue, PR or comment this task answers (a task from a GitHub automation).
  final GithubOrigin? origin;

  /// Set once a server restart interrupted the task and it was started again.
  final bool resumed;

  /// How many restarts interrupted it (the server fails it after too many).
  final int restarts;

  /// Images sent with the prompt: records only. The bytes are fetched with `crewmate_attachment`
  /// (`features/crew/chat/attachments.dart`).
  final List<TaskAttachment> attachments;

  /// The task changes what people see: it needs two captures the agent took to be done (`prove_ui`).
  final bool ui;

  /// The question the agent is blocked on (`ask_user`), while the task runs.
  final AskRequest? ask;

  /// What is read aloud when the task finishes (hostd's `spoken`, at most 300 characters, no code or paths).
  /// Empty: nothing is said. An older hostd sends nothing, which is the same.
  final String spoken;

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
    this.waitingFor,
    this.tz,
    this.origin,
    this.resumed = false,
    this.restarts = 0,
    this.attachments = const [],
    this.ui = false,
    this.ask,
    this.spoken = '',
  });

  /// The line of a queued task: a bare "Queued", or why it is not starting ("Queued · server busy").
  String get queuedLabel =>
      waitingFor == null ? 'Queued' : 'Queued · $waitingFor';

  /// The images the owner sent with the prompt (shown under it).
  List<TaskAttachment> get ownerImages => [
    for (final a in attachments)
      if (!a.fromAgent) a,
  ];

  /// The captures the agent took (shown on its card and under its reply).
  List<TaskAttachment> get agentImages => [
    for (final a in attachments)
      if (a.fromAgent) a,
  ];

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
    waitingFor: j['waiting_for'] as String?,
    tz: j['tz'] as String?,
    origin: j['origin'] == null
        ? null
        : GithubOrigin.fromJson((j['origin'] as Map).cast<String, dynamic>()),
    resumed: j['resumed'] as bool? ?? false,
    restarts: (j['restarts'] as num?)?.toInt() ?? 0,
    attachments: [
      for (final a in (j['attachments'] as List<dynamic>?) ?? const [])
        TaskAttachment.fromJson((a as Map).cast<String, dynamic>()),
    ],
    ui: j['ui'] as bool? ?? false,
    ask: j['ask'] == null
        ? null
        : AskRequest.fromJson((j['ask'] as Map).cast<String, dynamic>()),
    spoken: j['spoken'] as String? ?? '',
  );
}

/// Where a task from GitHub came from and where its reply goes (hostd `GithubOrigin`).
class GithubOrigin {
  final String repo;

  /// mention | issue_assigned | ci_failed | pr_review_comment
  final String kind;
  final int? number;
  final int? commentId;
  final String? branch;
  final int? replyCommentId;

  const GithubOrigin({
    required this.repo,
    required this.kind,
    this.number,
    this.commentId,
    this.branch,
    this.replyCommentId,
  });

  factory GithubOrigin.fromJson(Map<String, dynamic> j) => GithubOrigin(
    repo: j['repo'] as String? ?? '',
    kind: j['kind'] as String? ?? '',
    number: (j['number'] as num?)?.toInt(),
    commentId: (j['comment_id'] as num?)?.toInt(),
    branch: j['branch'] as String?,
    replyCommentId: (j['reply_comment_id'] as num?)?.toInt(),
  );
}
