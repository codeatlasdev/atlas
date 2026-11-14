import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';

void main() {
  group('CrewmateTask summarized contract', () {
    late Map<String, dynamic> fixture;

    setUpAll(() {
      final f = File('test/fixtures/crewmate_task_summarized.json');
      fixture = jsonDecode(f.readAsStringSync()) as Map<String, dynamic>;
    });

    test('summarized is true in fixture', () {
      expect(fixture['summarized'], isTrue);
    });

    test('CrewmateTask.fromJson parses summarized and replyFull', () {
      final task = CrewmateTask.fromJson(fixture);
      expect(task.summarized, isTrue);
      expect(task.replyFull, isNotNull);
      expect(task.replyFull!.length, greaterThan((task.result ?? '').length));
    });

    test('result is the short summary', () {
      final task = CrewmateTask.fromJson(fixture);
      expect(task.result, isNotEmpty);
      expect(task.result!.length, lessThan(task.replyFull!.length));
    });

    test('unsummarized task has summarized=false and no replyFull', () {
      final j = <String, dynamic>{
        'id': 't1',
        'ts': 0,
        'source': 'desktop',
        'prompt': 'hi',
        'state': 'done',
        'result': 'ok',
      };
      final task = CrewmateTask.fromJson(j);
      expect(task.summarized, isFalse);
      expect(task.replyFull, isNull);
    });
  });
}
