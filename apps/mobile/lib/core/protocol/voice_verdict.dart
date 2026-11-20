/// What hostd decided about a spoken answer to a card (`agent_approve_voice`, `crewmate_action_answer_voice`).
///
/// The phone sends only what it heard. hostd owns what the words mean and which calls may be answered by voice
/// at all (never Bypass, a rail, a destructive call or an untrusted source), so the safety rules live once, on the
/// server. The one rule here: the app NEVER turns words into an allow. A reply it cannot read, or a verdict it
/// does not know, is [VoiceVerdictKind.unclear] (nothing decided), never a decision. Plain Dart.
library;

enum VoiceVerdictKind {
  /// Allowed once; hostd already answered the approval.
  allow,

  /// Denied; hostd already answered the approval.
  deny,

  /// A choice card answered with [VoiceVerdict.choice]; hostd already answered the action.
  choice,

  /// The words were not a plain answer: [VoiceVerdict.comment] is what was heard, for the person to confirm by
  /// tap. Nothing is decided.
  comment,

  /// Voice may not answer this one ([VoiceVerdict.reason] says why). Nothing is decided.
  needsTap,

  /// Nothing usable was heard. Nothing is decided.
  unclear;

  /// hostd decided and answered: the card is done.
  bool get decided => this == allow || this == deny || this == choice;

  static VoiceVerdictKind parse(Object? wire) => switch (wire) {
    'allow' => allow,
    'deny' => deny,
    'choice' => choice,
    'comment' => comment,
    'needs_tap' => needsTap,
    _ => unclear,
  };
}

/// Why voice may not answer (`needs_tap.reason`); null for a card kind voice never answers (a secret, a refused
/// task).
enum VoiceRefusal {
  /// The crewmate is in Bypass.
  bypass,

  /// A rail (a protected path, a task that never auto-runs) holds the prompt.
  rail,

  /// The call is destructive (`rm -rf`, `git push --force`, `DROP TABLE`, ...).
  destructive,

  /// The words came from a source hostd does not trust for answering.
  untrusted;

  static VoiceRefusal? parse(Object? wire) => switch (wire) {
    'bypass' => bypass,
    'rail' => rail,
    'destructive' => destructive,
    'untrusted' => untrusted,
    _ => null,
  };
}

class VoiceVerdict {
  const VoiceVerdict(this.kind, {this.choice, this.comment, this.reason});

  final VoiceVerdictKind kind;

  /// For [VoiceVerdictKind.choice]: the option's text as the card has it.
  final String? choice;

  /// For [VoiceVerdictKind.comment]: the transcript.
  final String? comment;

  /// For [VoiceVerdictKind.needsTap].
  final VoiceRefusal? reason;

  /// Nothing was decided and nothing was understood.
  static const unclear = VoiceVerdict(VoiceVerdictKind.unclear);

  /// The `data` of hostd's reply. A reply of another shape reads as [unclear].
  factory VoiceVerdict.fromJson(Object? data) {
    if (data is! Map) return unclear;
    return VoiceVerdict(
      VoiceVerdictKind.parse(data['verdict']),
      choice: data['choice'] as String?,
      comment: data['comment'] as String?,
      reason: VoiceRefusal.parse(data['reason']),
    );
  }
}

/// `en` or `pt`: the word tables hostd answers with (`lang` of both commands).
String voiceLang(String languageCode) => languageCode == 'pt' ? 'pt' : 'en';
