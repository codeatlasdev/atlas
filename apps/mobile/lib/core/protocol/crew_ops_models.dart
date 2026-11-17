/// Models of the crew operations the desktop has in Settings and the phone now has too: the audit
/// (`crewmate_audit` / `crew_audit`), token usage (`crewmate_usage`), engine limits (`engine_limits`),
/// crew settings (`crew_settings_get` / `crew_settings_set`) and cron routines (`crewmate_routine_set`).
/// Shapes are hostd's; `test/crew_ops_contract_test.dart` reads the real replies as fixtures. Plain Dart.
library;

import 'dart:convert';

import 'package:atlas_mobile/core/protocol/agent_registry.dart';

// ─── audit ────────────────────────────────────────────────────────────────────

/// The desktop's filter groups of the audit kinds (`apps/desktop/src/lib/crew/settings/audit.ts`).
enum AuditGroup {
  tools('Tool calls'),
  approvals('Approvals'),
  tasks('Tasks'),
  config('Settings');

  const AuditGroup(this.label);
  final String label;
}

/// Every kind hostd writes for a crewmate: its label and filter group.
const _auditKinds = <String, (String, AuditGroup)>{
  'tool_call': ('Tool call', AuditGroup.tools),
  'approval_asked': ('Approval asked', AuditGroup.approvals),
  'approval_decision': ('Approval decided', AuditGroup.approvals),
  'task_queued': ('Task queued', AuditGroup.tasks),
  'task_started': ('Task started', AuditGroup.tasks),
  'task_finished': ('Task finished', AuditGroup.tasks),
  'config_changed': ('Setting changed', AuditGroup.config),
  'secret_set': ('Secret set', AuditGroup.config),
  'engine_login_started': ('Sign-in started', AuditGroup.config),
};

String auditKindLabel(String kind) => _auditKinds[kind]?.$1 ?? kind;

const _listMax = 120;
String _clip(String s) =>
    s.length > _listMax ? '${s.substring(0, _listMax)}…' : s;

/// A tool's input as one line: a lone string argument as is (a command, a path), else compact JSON.
String _inputText(Object? input) {
  if (input == null) return '';
  if (input is String) return input;
  if (input is Map && input.length == 1 && input.values.first is String) {
    return input.values.first as String;
  }
  return jsonEncode(input);
}

/// Who answered an approval, readable: `desktop`, `phone:<node>`, `telegram:<chat>`, `none`.
String _answerer(String? by) {
  if (by == null || by.isEmpty || by == 'none') return 'nobody answered';
  if (by.startsWith('phone:')) {
    final node = by.substring(6);
    return 'answered by phone ${node.length > 8 ? node.substring(0, 8) : node}…';
  }
  if (by.startsWith('telegram:')) {
    return 'answered by Telegram chat ${by.substring(9)}';
  }
  return 'answered by $by';
}

/// Who asked for a sign-in, readable: `desktop`, `phone:<node>`, `telegram:<chat>`.
String _requester(String? by) {
  if (by == null || by.isEmpty) return 'by someone';
  if (by.startsWith('phone:')) {
    final node = by.substring(6);
    return 'from phone ${node.length > 8 ? node.substring(0, 8) : node}…';
  }
  if (by.startsWith('telegram:')) {
    return 'from Telegram chat ${by.substring(9)}';
  }
  return 'from $by';
}

/// One line of the audit log: the common keys plus whatever the kind adds.
class AuditEntry {
  const AuditEntry(this.raw);

  /// The entry as hostd sent it; everything else reads from here.
  final Map<String, dynamic> raw;

  factory AuditEntry.fromJson(Map<String, dynamic> j) => AuditEntry(j);

  int get seq => (raw['seq'] as num).toInt();
  int get ts => (raw['ts'] as num).toInt();
  String get kind => raw['kind'] as String;
  String? get crewmateId => raw['crewmate_id'] as String?;
  String? get taskId => raw['task_id'] as String?;
  AuditGroup? get group => _auditKinds[kind]?.$2;

  /// One line for the list (what the desktop shows).
  String get summary {
    String s(String k) => raw[k] as String? ?? '';
    switch (kind) {
      case 'tool_call':
        final input = _inputText(raw['tool_input']);
        final name = raw['tool_name'] as String? ?? '?';
        return _clip(input.isEmpty ? name : '$name: $input');
      case 'approval_asked':
        final input = _inputText(raw['tool_input']);
        final name = raw['tool_name'] as String? ?? '?';
        return _clip('Asked to run $name${input.isEmpty ? '' : ': $input'}');
      case 'approval_decision':
        final verb = switch (raw['decision']) {
          'allow' => 'Allowed',
          'deny' => 'Denied',
          'timeout' => 'Timed out',
          _ => 'Decided',
        };
        final tool = raw['tool_name'] as String? ?? 'a tool';
        return '$verb $tool (${_answerer(raw['by'] as String?)})';
      case 'task_queued':
        final src = raw['source'] as String? ?? '?';
        return _clip('Task queued ($src): ${s('prompt')}');
      case 'task_started':
        return _clip('Task started: ${s('prompt')}');
      case 'task_finished':
        final err = raw['error'];
        return 'Task finished: ${raw['state'] ?? '?'}${err != null ? ' ($err)' : ''}';
      case 'config_changed':
        return _clip(
          'Changed ${raw['field'] ?? '?'}: ${jsonEncode(raw['from'])} → ${jsonEncode(raw['to'])}',
        );
      case 'secret_set':
        return 'Set secret ${raw['name'] ?? '?'}';
      // The crew-wide log.
      case 'paused':
        return 'Crew paused';
      case 'resumed':
        return 'Crew resumed';
      case 'crewmate_created':
        return 'Crewmate ${raw['name'] ?? '?'} created';
      case 'crewmate_deleted':
        return 'Crewmate ${raw['name'] ?? '?'} deleted';
      case 'engine_login_started':
        return 'Sign-in of ${raw['engine'] ?? '?'} started (${_requester(raw['by'] as String?)})';
      default:
        return kind;
    }
  }

  /// What Copy puts on the clipboard: the time, then the whole entry (nothing is shortened).
  String get copyText =>
      '${DateTime.fromMillisecondsSinceEpoch(ts * 1000, isUtc: true).toIso8601String()}\n'
      '${const JsonEncoder.withIndent('  ').convert(raw)}';
}

/// The entries of a `crewmate_audit` / `crew_audit` reply, newest first as hostd sends them. A reply
/// without them (an older server) or a malformed row gives none.
List<AuditEntry> parseAudit(Map<String, dynamic> reply) {
  final rows = reply['entries'];
  if (rows is! List) return const [];
  return [
    for (final r in rows)
      if (r is Map && r['seq'] is num && r['ts'] is num && r['kind'] is String)
        AuditEntry.fromJson(r.cast<String, dynamic>()),
  ];
}

/// Only the entries of one group (`null` = all).
List<AuditEntry> filterAudit(List<AuditEntry> entries, AuditGroup? group) =>
    group == null
    ? entries
    : [
        for (final e in entries)
          if (e.group == group) e,
      ];

// ─── usage ────────────────────────────────────────────────────────────────────

class UsageRecord {
  const UsageRecord({
    required this.ts,
    this.engine,
    this.model,
    this.inputTokens,
    this.outputTokens,
    this.cacheRead,
    this.cacheWrite,
    this.costUsd,
    this.durationMs,
    this.crewmateId,
  });

  final int ts;
  final String? engine;
  final String? model;
  final int? inputTokens;
  final int? outputTokens;
  final int? cacheRead;
  final int? cacheWrite;
  final double? costUsd;
  final int? durationMs;
  final String? crewmateId;

  factory UsageRecord.fromJson(Map<String, dynamic> j) => UsageRecord(
    ts: (j['ts'] as num?)?.toInt() ?? 0,
    engine: j['engine'] as String?,
    model: j['model'] as String?,
    inputTokens: (j['input_tokens'] as num?)?.toInt(),
    outputTokens: (j['output_tokens'] as num?)?.toInt(),
    cacheRead: (j['cache_read'] as num?)?.toInt(),
    cacheWrite: (j['cache_write'] as num?)?.toInt(),
    costUsd: (j['cost_usd'] as num?)?.toDouble(),
    durationMs: (j['duration_ms'] as num?)?.toInt(),
    crewmateId: j['crewmate_id'] as String?,
  );

  int get tokens => (inputTokens ?? 0) + (outputTokens ?? 0);
}

List<UsageRecord> parseUsage(Map<String, dynamic> reply) {
  final rows = reply['records'];
  if (rows is! List) return const [];
  return [
    for (final r in rows)
      if (r is Map) UsageRecord.fromJson(r.cast<String, dynamic>()),
  ];
}

/// Tokens (and cost) since local midnight, 7 days ago and 30 days ago.
class UsageTotals {
  const UsageTotals({
    required this.today,
    required this.week,
    required this.month,
    required this.cost,
  });
  final int today;
  final int week;
  final int month;

  /// Cost of the records in the month, the ones that reported one.
  final double cost;
}

const usageDays = 30;

UsageTotals usageTotals(List<UsageRecord> records, DateTime now) {
  final midnight = DateTime(now.year, now.month, now.day);
  int since(DateTime from) => records
      .where((r) => r.ts * 1000 >= from.millisecondsSinceEpoch)
      .fold(0, (sum, r) => sum + r.tokens);
  final monthAgo = now.subtract(const Duration(days: usageDays));
  return UsageTotals(
    today: since(midnight),
    week: since(now.subtract(const Duration(days: 7))),
    month: since(monthAgo),
    cost: records
        .where((r) => r.ts * 1000 >= monthAgo.millisecondsSinceEpoch)
        .fold(0.0, (sum, r) => sum + (r.costUsd ?? 0)),
  );
}

/// Tokens per day for the last 30 days, oldest first (the last bucket is "now" back to 24 h ago).
List<int> usageBuckets(List<UsageRecord> records, DateTime now) {
  const dayMs = 24 * 60 * 60 * 1000;
  final b = List<int>.filled(usageDays, 0);
  for (final r in records) {
    final idx = ((now.millisecondsSinceEpoch - r.ts * 1000) / dayMs).floor();
    if (idx >= 0 && idx < usageDays) b[usageDays - 1 - idx] += r.tokens;
  }
  return b;
}

/// 1234 → "1.2k", 2 500 000 → "2.5M"; below 1000 the plain number.
String fmtTokens(int n) {
  if (n >= 1000000) return '${(n / 1000000).toStringAsFixed(1)}M';
  if (n >= 1000) return '${(n / 1000).toStringAsFixed(1)}k';
  return '$n';
}

// ─── engine limits ────────────────────────────────────────────────────────────

/// `engine_limits`: what each engine CLI exposes about its quota. Today none does, and hostd says so.
class EngineLimits {
  const EngineLimits({required this.message, required this.limits});
  final String message;

  /// Engine → what it reported, as readable JSON text.
  final Map<String, String> limits;

  factory EngineLimits.fromJson(Map<String, dynamic> j) => EngineLimits(
    message: j['message'] as String? ?? '',
    limits: {
      for (final e in ((j['limits'] as Map?) ?? const {}).entries)
        e.key as String: e.value is String
            ? e.value as String
            : const JsonEncoder.withIndent('  ').convert(e.value),
    },
  );
}

// ─── crew settings ────────────────────────────────────────────────────────────

/// `crew_settings_get`: the crew-wide reply summary switch and its model (null = hostd's default).
class CrewSettings {
  const CrewSettings({required this.shortReplies, this.shortRepliesModel});
  final bool shortReplies;
  final String? shortRepliesModel;

  factory CrewSettings.fromJson(Map<String, dynamic> j) => CrewSettings(
    shortReplies: j['short_replies'] as bool? ?? true,
    shortRepliesModel: j['short_replies_model'] as String?,
  );

  /// The `crew_settings_set` args; an empty model clears it (hostd falls back to the default).
  Map<String, dynamic> toArgs() => {
    'short_replies': shortReplies,
    'short_replies_model': (shortRepliesModel ?? '').isEmpty
        ? null
        : shortRepliesModel,
  };
}

// ─── engine sign-in ───────────────────────────────────────────────────────────

/// The engines a crew server can sign in (every built-in agent), in the order the settings list shows them.
final engineLabels = <String, String>{
  for (final a in builtinAgents) a.id: a.label,
};

/// One row of `crew_engine_status`: `state` is `signed_in`, `needs_signin`, `not_installed` or `unknown`
/// (the check did not answer); `reason` is the auth error a failed task reported, when that flipped it.
class EngineStatus {
  const EngineStatus({required this.engine, required this.state, this.reason});
  final String engine;
  final String state;
  final String? reason;

  factory EngineStatus.fromJson(Map<String, dynamic> j) => EngineStatus(
    engine: j['engine'] as String,
    state: j['state'] as String,
    reason: j['reason'] as String?,
  );

  String get label => engineLabels[engine] ?? engine;
  bool get needsSignin => state == 'needs_signin';

  /// What the row says under the name.
  String get text => switch (state) {
    'signed_in' => 'Signed in',
    'needs_signin' => 'Needs sign-in',
    'not_installed' => 'Not installed on this server',
    _ => 'Could not check',
  };
}

/// The `engines` of a `crew_engine_status` reply: the ones the phone knows in [engineLabels] order, then any
/// engine a newer hostd adds, in the order it sent them (the sort is by a key that includes the position).
List<EngineStatus> parseEngineStatus(Map<String, dynamic> data) {
  final rows = [
    for (final r in (data['engines'] as List? ?? const []))
      if (r is Map) EngineStatus.fromJson(r.cast<String, dynamic>()),
  ];
  final order = engineLabels.keys.toList();
  int rank(EngineStatus s) {
    final i = order.indexOf(s.engine);
    return i < 0 ? order.length : i;
  }

  final keyed = [
    for (var i = 0; i < rows.length; i++) (rank(rows[i]), i, rows[i]),
  ]..sort((a, b) => a.$1 != b.$1 ? a.$1.compareTo(b.$1) : a.$2.compareTo(b.$2));
  return [for (final k in keyed) k.$3];
}

/// The `crew_engine_login` reply: the terminal to open.
class EngineLogin {
  const EngineLogin({required this.engine, required this.pty});
  final String engine;
  final String pty;

  factory EngineLogin.fromJson(Map<String, dynamic> j) =>
      EngineLogin(engine: j['engine'] as String, pty: j['pty'] as String);
}

// ─── routines ─────────────────────────────────────────────────────────────────

/// A cron routine of a crewmate: a prompt it runs on a schedule (`source: "routine"`).
class Routine {
  const Routine({
    required this.id,
    required this.cron,
    required this.prompt,
    required this.enabled,
    this.lastRun,
  });

  factory Routine.create({
    required String id,
    required String cron,
    required String prompt,
  }) => Routine(id: id, cron: cron, prompt: prompt, enabled: true);

  final String id;
  final String cron;
  final String prompt;
  final bool enabled;
  final int? lastRun;

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
    'last_run': lastRun,
  };

  Routine copyWith({String? cron, String? prompt, bool? enabled}) => Routine(
    id: id,
    cron: cron ?? this.cron,
    prompt: prompt ?? this.prompt,
    enabled: enabled ?? this.enabled,
    lastRun: lastRun,
  );
}

/// [list] with [r] replacing the routine of the same id, or appended when new.
List<Routine> upsertRoutine(List<Routine> list, Routine r) =>
    list.any((x) => x.id == r.id)
    ? [for (final x in list) x.id == r.id ? r : x]
    : [...list, r];

List<Routine> removeRoutine(List<Routine> list, String id) => [
  for (final x in list)
    if (x.id != id) x,
];

/// Five whitespace-separated cron fields. A shape check to catch a typo before the call; hostd's
/// parser is the real validator and its error is shown when it refuses.
bool looksLikeCron(String s) {
  final parts = s.trim().split(RegExp(r'\s+'));
  return parts.length == 5 &&
      parts.every((p) => RegExp(r'^[0-9*/,\-A-Za-z?#LW]+$').hasMatch(p));
}
