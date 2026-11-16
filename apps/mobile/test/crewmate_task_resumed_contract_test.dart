import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_task.dart';

void main() {
  // The task as hostd serializes it after a restart interrupted it (#199): written by the Rust test
  // crewmate_task_resumed_fixture_is_real_payload (UPDATE_FIXTURES=1), never by hand.
  group('CrewmateTask resumed contract', () {
    late Map<String, dynamic> fixture;

    setUpAll(() {
      final f = File('test/fixtures/crewmate_task_resumed.json');
      fixture = jsonDecode(f.readAsStringSync()) as Map<String, dynamic>;
    });

    test('CrewmateTask.fromJson parses resumed and restarts', () {
      final task = CrewmateTask.fromJson(fixture);
      expect(task.resumed, isTrue);
      expect(task.restarts, 1);
      expect(task.state, 'queued');
      expect(task.steps.any((s) => s.text.contains('restarted')), isTrue);
    });

    test('a task no restart touched is not resumed', () {
      final task = CrewmateTask.fromJson(<String, dynamic>{
        'id': 't1',
        'ts': 0,
        'source': 'desktop',
        'prompt': 'hi',
        'state': 'done',
      });
      expect(task.resumed, isFalse);
      expect(task.restarts, 0);
    });
  });
}
