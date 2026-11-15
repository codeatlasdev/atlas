// Contract: CrewmateTask parses `ask` from the real Rust task while an agent waits in `ask_user`
// (fixtures/crewmate_task_ask.json, written by atlas-host's
// `ask_user_fixtures_are_real_payloads_and_the_phones_answer_wakes_the_agent`), and the answer the
// phone sends is the `crewmate_answer {id, ask_id, key|text}` hostd parses.
// Incident (PhoneCrewAudit gap 3): the agent blocked up to 30 min on a question the phone never showed.
// Regenerate: UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::mobile_payloads
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';

void main() {
  late Map<String, dynamic> raw;

  setUpAll(() {
    final fixture = jsonDecode(
      File('test/fixtures/crewmate_task_ask.json').readAsStringSync(),
    ) as Map<String, dynamic>;
    raw = ((fixture['tasks'] as List).first as Map).cast<String, dynamic>();
  });

  test('the open question, its options and free-text flag parse', () {
    final task = CrewmateTask.fromJson(raw);
    final ask = task.ask;
    expect(ask, isNotNull, reason: 'Rust Task.ask must parse');
    expect(ask!.id, 'ask-fixture-1');
    expect(ask.question, 'Deploy to which environment?');
    expect(ask.allowText, isTrue);
    expect(ask.options.map((o) => o.key), ['stg', 'prod']);
    expect(ask.options.first.title, 'Staging');
    expect(ask.options.first.subtitle, 'safe');
    expect(ask.options.last.subtitle, isNull);
    expect(ask.isOpen, isTrue);
    expect(task.state, 'running');
  });

  test('an answered ask is no longer open', () {
    final answered = {
      ...raw,
      'ask': {
        ...(raw['ask'] as Map<String, dynamic>),
        'answered': {'kind': 'key', 'value': 'stg'},
      },
    };
    final ask = CrewmateTask.fromJson(answered).ask!;
    expect(ask.isOpen, isFalse);
    expect(ask.answeredWith, 'stg');
  });

  test('a task without an ask has none', () {
    final plain = {...raw}..remove('ask');
    expect(CrewmateTask.fromJson(plain).ask, isNull);
  });

  test('the answer args are what hostd parses: key for an option, text for free text', () {
    final ask = CrewmateTask.fromJson(raw).ask!;
    expect(ask.answerArgs('crewmate-1', key: 'prod'), {
      'id': 'crewmate-1',
      'ask_id': 'ask-fixture-1',
      'key': 'prod',
    });
    expect(ask.answerArgs('crewmate-1', text: 'canary'), {
      'id': 'crewmate-1',
      'ask_id': 'ask-fixture-1',
      'text': 'canary',
    });
    // Neither or both is no answer: hostd refuses "key or text required".
    expect(() => ask.answerArgs('c'), throwsArgumentError);
    expect(() => ask.answerArgs('c', key: 'a', text: 'b'), throwsArgumentError);
  });
}
