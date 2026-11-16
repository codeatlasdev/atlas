/// An `ask_user` question a crewmate raised, and the answer it takes back.
library;

/// One choice of an `ask_user` question.
class AskOption {
  final String key;
  final String title;
  final String? subtitle;
  const AskOption({required this.key, required this.title, this.subtitle});

  factory AskOption.fromJson(Map<String, dynamic> j) => AskOption(
    key: j['key'] as String? ?? '',
    title: j['title'] as String? ?? '',
    subtitle: j['subtitle'] as String?,
  );
}

/// The question an agent is blocked on (`ask_user`, hostd `AskRequest`): answered with `crewmate_answer`.
class AskRequest {
  final String id;
  final String question;
  final List<AskOption> options;
  final bool allowText;

  /// The answer once given (`{kind: key|text, value}`): the question is closed.
  final String? answeredWith;

  const AskRequest({
    required this.id,
    required this.question,
    this.options = const [],
    this.allowText = false,
    this.answeredWith,
  });

  bool get isOpen => answeredWith == null;

  factory AskRequest.fromJson(Map<String, dynamic> j) => AskRequest(
    id: j['id'] as String? ?? '',
    question: j['question'] as String? ?? '',
    options: [
      for (final o in (j['options'] as List<dynamic>?) ?? const [])
        AskOption.fromJson((o as Map).cast<String, dynamic>()),
    ],
    allowText: j['allow_text'] as bool? ?? false,
    answeredWith: j['answered'] == null
        ? null
        : (j['answered'] as Map)['value'] as String?,
  );

  /// The `crewmate_answer` args: exactly one of [key] (an option) or [text] (free text).
  Map<String, dynamic> answerArgs(
    String crewmateId, {
    String? key,
    String? text,
  }) {
    if ((key == null) == (text == null)) {
      throw ArgumentError('give exactly one of key or text');
    }
    return {'id': crewmateId, 'ask_id': id, 'key': ?key, 'text': ?text};
  }
}
