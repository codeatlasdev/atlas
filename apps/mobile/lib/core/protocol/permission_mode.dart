/// A crewmate's permission mode and how a tool approval is answered (`mode` of `crewmate_list`,
/// `crewmate_set_approval`, `crew_settings_*`; `scope` and `comment` of `agent_approve`). The wire contract is
/// hostd's (`crew/approval.rs`); `test/permission_modes_contract_test.dart` reads its real payloads.
library;

/// What a crewmate may run without asking. Wire values are hostd's: `ask`, `edits`, `bypass`.
enum PermissionMode {
  /// Nothing runs without an answer.
  ask,

  /// File edits inside the crewmate's worktree run; everything else asks.
  edits,

  /// Every tool call runs, except what a rail holds. The only mode drawn in the warning colour.
  bypass;

  String get wire => name;

  /// The mode of a wire value; null for one this app does not know (a newer hostd).
  static PermissionMode? tryParse(Object? wire) {
    for (final m in values) {
      if (m.name == wire) return m;
    }
    return null;
  }

  /// The mode of a crewmate entry: `mode`, else what an older hostd's `approval` says (`auto` was Bypass).
  static PermissionMode ofEntry(Map<String, dynamic> j) =>
      tryParse(j['mode']) ?? (j['approval'] == 'auto' ? bypass : ask);
}

/// How far an Allow reaches: this call, or the same kind of call for the rest of the task. There is no "always".
enum ApprovalScope {
  once,
  task;

  String get wire => name;
}

/// Why hostd forced a question that no mode removes (`pending_approval.rail`).
enum ApprovalRail {
  /// `.git/` internals, agent config, shell rc files, `.env*`, key files.
  protectedPath('protected_path'),

  /// A GitHub or webhook task that does not set `allow_auto`.
  noAuto('no_auto');

  const ApprovalRail(this.wire);
  final String wire;

  static ApprovalRail? tryParse(Object? wire) {
    for (final r in values) {
      if (r.wire == wire) return r;
    }
    return null;
  }
}

/// The longest comment hostd takes (`agent_approve`, trimmed); a longer one is cut here instead of refused there.
const approvalCommentMax = 2000;

/// The `agent_approve` arguments of an answer. `scope` goes only when it is not the default and never for a
/// call a rail holds (hostd ignores it there; saying so would suggest a grant that does not exist); `comment`
/// only when there is one after trimming.
Map<String, Object?> approvalAnswerArgs({
  required String pane,
  required String req,
  required bool allow,
  ApprovalScope scope = ApprovalScope.once,
  ApprovalRail? rail,
  String? comment,
}) {
  var text = (comment ?? '').trim();
  if (text.length > approvalCommentMax) {
    text = text.substring(0, approvalCommentMax).trimRight();
  }
  return {
    'pane': pane,
    'req': req,
    'allow': allow,
    if (allow && scope == ApprovalScope.task && rail == null)
      'scope': scope.wire,
    if (text.isNotEmpty) 'comment': text,
  };
}

/// A refusal hostd sent for a request: its stable `code` and `args` (`REPLY_CODES`), and the English `error`
/// text an older client shows. Null `code` is a plain text failure.
class HostRefusal implements Exception {
  const HostRefusal(this.text, {this.code, this.args = const {}});

  final String text;
  final String? code;
  final Map<String, dynamic> args;

  /// The refusal in a failed reply (`{ok: false, error, code?, args?}`).
  factory HostRefusal.fromReply(Map<String, dynamic> reply) => HostRefusal(
    reply['error'] as String? ?? 'the server refused',
    code: reply['code'] as String?,
    args: (reply['args'] as Map?)?.cast<String, dynamic>() ?? const {},
  );

  /// hostd's own text for a phone paired without the scope: `forbidden: approve`.
  bool get missingApproveScope => text == 'forbidden: approve';

  @override
  String toString() => text;
}

const approvalBypassDisabled = 'crew.approval.bypass_disabled';
const approvalUnsupportedEngine = 'crew.approval.unsupported_engine';
const approvalForbidden = 'crew.approval.forbidden';
