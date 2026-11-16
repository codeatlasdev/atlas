/// Atlas Crew protocol models (hostd `crewmate_*` payloads): the crewmate list entry and
/// what it carries (approval, waiting/running task, last result), and the pairing invite
/// pushed by a crew server. Plain Dart, no codegen.
library;

import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/protocol/crew_triggers.dart';

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

/// A task parked on a card (`crewmate_list.waiting_task`): the card and the captures the agent took, as records.
class CrewmateWaitingTask {
  const CrewmateWaitingTask({
    required this.id,
    required this.action,
    this.result,
    this.attachments = const [],
  });
  final String id;
  final TaskAction action;

  /// What the agent said before the card.
  final String? result;
  final List<TaskAttachment> attachments;

  /// Kinds that offer Retry / Override: the list card can answer them without opening the chat.
  bool get retryable => action.kind == 'refused' || action.kind == 'prove_ui';

  factory CrewmateWaitingTask.fromJson(Map<String, dynamic> j) =>
      CrewmateWaitingTask(
        id: j['id'] as String? ?? '',
        action: TaskAction.fromJson(
          (j['action'] as Map).cast<String, dynamic>(),
        ),
        result: j['result'] as String?,
        attachments: [
          for (final a in (j['attachments'] as List<dynamic>?) ?? const [])
            TaskAttachment.fromJson((a as Map).cast<String, dynamic>()),
        ],
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
    this.waitingTask,
    this.triggers = const [],
    this.routines = const [],
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

  /// A task parked on a card (refused, unproven UI, a question): shown in the crew list's "Captain's call".
  final CrewmateWaitingTask? waitingTask;
  final List<Trigger> triggers;

  /// Cron routines (`crewmate_list.routines`); `crewmate_routine_set` replaces the whole list.
  final List<Routine> routines;
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
    waitingTask: j['waiting_task'] == null
        ? null
        : CrewmateWaitingTask.fromJson((j['waiting_task'] as Map).cast()),
    triggers: [
      for (final t in (j['triggers'] as List<dynamic>?) ?? const [])
        Trigger.fromJson((t as Map).cast()),
    ],
    routines: [
      for (final r in (j['routines'] as List<dynamic>?) ?? const [])
        Routine.fromJson((r as Map).cast()),
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
