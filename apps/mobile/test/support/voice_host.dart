import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/transport/transport.dart';

/// hostd's real replies for #648 (`crew_voice_answers.json`, written by its serializer).
final voiceAnswers = jsonDecode(
  File('test/fixtures/crew_voice_answers.json').readAsStringSync(),
) as Map<String, dynamic>;

Map<String, dynamic> voiceVerdict(String group, String key) =>
    ((voiceAnswers[group] as Map)[key] as Map).cast<String, dynamic>();

/// One crewmate with one pending approval or one choice card, voice on, and a log of every command it was sent.
/// The verdict of a spoken answer is whatever the test says hostd answered ([approveVerdict], [answerVerdict]),
/// taken from hostd's own fixture; the app must show it and decide nothing more.
class VoiceHost implements Transport {
  VoiceHost({this.voiceEnabled = true, this.spokenReplies = false}) {
    crewmate = {
      ...((voiceAnswers['crewmate'] as Map).cast<String, dynamic>()),
      'id': id,
      'spoken_replies': spokenReplies,
    };
  }

  static const id = 'cm-1';
  static const approvalReq = 'req-1';
  static const approvalPane = 'crew:cm-1:task-1';

  final bool voiceEnabled;
  final bool spokenReplies;
  late final Map<String, dynamic> crewmate;

  /// Newest first, as `crewmate_tasks` answers.
  final List<Map<String, dynamic>> thread = [];

  /// A pending approval on the crewmate (null: none).
  Map<String, dynamic>? approval;

  /// What `agent_approve_voice` / `crewmate_action_answer_voice` answer (the `data`), or a refusal.
  Map<String, dynamic>? approveVerdict;
  Map<String, dynamic>? answerVerdict;
  String? voiceRefusal;

  final calls = <({String cmd, Map<String, dynamic> args})>[];
  final _in = StreamController<String>.broadcast();

  Iterable<Map<String, dynamic>> sent(String cmd) =>
      calls.where((c) => c.cmd == cmd).map((c) => c.args);

  @override
  Stream<String> get incomingLines => _in.stream;

  @override
  Future<void> close() async {
    if (!_in.isClosed) await _in.close();
  }

  void push(Map<String, Object?> event) {
    if (!_in.isClosed) _in.add(jsonEncode(event));
  }

  /// hostd's push for a task that changed.
  void pushTask(String taskId) => push({
    'ev': 'crewmate_task',
    'crewmate_id': id,
    'task': {'id': taskId},
  });

  static Map<String, dynamic> task(
    String id,
    String state, {
    String spoken = '',
    int ts = 1762000000,
    Map<String, dynamic>? action,
  }) => {
    'id': id,
    'ts': ts,
    'source': 'mobile',
    'prompt': 'do $id',
    'state': state,
    if (state == 'done') 'result': 'The full reply of $id.',
    'spoken': spoken,
    if (action != null) 'action': action,
  };

  Map<String, dynamic> _row() => {
    ...crewmate,
    'state': approval != null ? 'needs_you' : 'idle',
    if (approval != null) 'pending_approval': approval,
  };

  @override
  Future<void> sendLine(String line) async {
    final m = jsonDecode(line) as Map<String, dynamic>;
    final cmd = m['cmd'] as String;
    final args = (m['args'] as Map?)?.cast<String, dynamic>() ?? const {};
    calls.add((cmd: cmd, args: args));
    void answer(Map<String, Object?> body) => scheduleMicrotask(() {
      if (!_in.isClosed) _in.add(jsonEncode({'id': m['id'], ...body}));
    });
    Map<String, Object?> ok(Object data) => {'ok': true, 'data': data};

    switch (cmd) {
      case 'info':
        return answer(ok({'proto': kAppProto}));
      case 'crew_servers':
        return answer(ok({'servers': []}));
      case 'crew_voice_status':
        return answer(ok({'enabled': voiceEnabled}));
      case 'crewmate_list':
        return answer(
          ok({
            'host_name': 'Mac',
            'crewmates': [_row()],
          }),
        );
      case 'crewmate_tasks':
        return answer(ok({'tasks': thread}));
      case 'crewmate_update':
        if (args['spoken_replies'] is bool) {
          crewmate['spoken_replies'] = args['spoken_replies'];
        }
        return answer(ok(_row()));
      case 'agent_approve_voice':
        if (voiceRefusal != null) {
          return answer({'ok': false, 'error': voiceRefusal});
        }
        return answer(ok(approveVerdict ?? {'verdict': 'unclear'}));
      case 'crewmate_action_answer_voice':
        if (voiceRefusal != null) {
          return answer({'ok': false, 'error': voiceRefusal});
        }
        return answer(ok(answerVerdict ?? {'verdict': 'unclear'}));
      default:
        return answer(ok({}));
    }
  }
}
