import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:atlas_mobile/core/protocol/models.dart';

void main() {
  group('TaskAction contract', () {
    late List<dynamic> fixture;

    setUpAll(() {
      final f = File('test/fixtures/task_actions.json');
      fixture = jsonDecode(f.readAsStringSync()) as List;
    });

    test('connect_github parses correctly', () {
      final j = fixture.firstWhere(
        (e) => e['kind'] == 'connect_github',
      ) as Map<String, dynamic>;
      final a = TaskAction.fromJson(j);
      expect(a.kind, 'connect_github');
      expect(a.reason, 'to clone owner/repo');
    });

    test('provide_secret parses correctly', () {
      final j = fixture.firstWhere(
        (e) => e['kind'] == 'provide_secret',
      ) as Map<String, dynamic>;
      final a = TaskAction.fromJson(j);
      expect(a.kind, 'provide_secret');
      expect(a.name, 'API_KEY');
      expect(a.label, 'API Key');
      expect(a.reason, 'needed for the API');
    });

    test('input parses with placeholder', () {
      final j = fixture.firstWhere(
        (e) => e['kind'] == 'input',
      ) as Map<String, dynamic>;
      final a = TaskAction.fromJson(j);
      expect(a.kind, 'input');
      expect(a.label, 'Enter value');
      expect(a.placeholder, 'e.g. my-value');
    });

    test('choose parses options', () {
      final j = fixture.firstWhere(
        (e) => e['kind'] == 'choose',
      ) as Map<String, dynamic>;
      final a = TaskAction.fromJson(j);
      expect(a.kind, 'choose');
      expect(a.question, 'Which deployment target?');
      expect(a.options, ['staging', 'production']);
    });

    test('sign_in_engine parses engine field', () {
      final j = fixture.firstWhere(
        (e) => e['kind'] == 'sign_in_engine',
      ) as Map<String, dynamic>;
      final a = TaskAction.fromJson(j);
      expect(a.kind, 'sign_in_engine');
      expect(a.engine, 'omp');
    });

    test('BotTask with waiting_action state parses action', () {
      final j = <String, dynamic>{
        'id': 'task-1',
        'ts': 1700000000000,
        'source': 'desktop',
        'prompt': 'test',
        'state': 'waiting_action',
        'action': {'kind': 'connect_github', 'reason': 'to clone x/y'},
      };
      final t = BotTask.fromJson(j);
      expect(t.state, 'waiting_action');
      expect(t.action?.kind, 'connect_github');
      expect(t.action?.reason, 'to clone x/y');
    });
  });
}
