// Contract: the phone recognizes hostd's real `crewmate_task` push for the right crewmate.
// Incident: the task list read e['bot']['id'] while hostd sends a flat id, so a crewmate's
// tasks never refreshed on a real push (only the demo, which sent the same wrong shape).
// Regenerate fixture: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_task_event_fixture_is_real_payload
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/features/crew/crew_provider.dart';

void main() {
  final event = (jsonDecode(
    File('test/fixtures/crewmate_task_event.json').readAsStringSync(),
  ) as Map).cast<String, dynamic>();
  final id = event['crewmate_id'] as String;

  test('the real push matches its own crewmate', () {
    expect(isTaskPushFor(event, id), isTrue);
  });

  test('another crewmate\'s push does not refresh this one', () {
    expect(isTaskPushFor(event, '$id-other'), isFalse);
  });

  test('a crewmate_updated push is not a task change', () {
    expect(isTaskPushFor({...event, 'ev': 'crewmate_updated'}, id), isFalse);
  });
}
