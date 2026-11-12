// Contract: BotTask parses title/preparing/tz, BotStep parses group,
// and TaskAction parses add_trigger kind from the real Rust bot_tasks fixture.
// Incident: new fields absent from mobile model; ChatBoard needed them for live steps.
// Regenerate fixture: UPDATE_FIXTURES=1 cargo test -p atlas-host bot_tasks_json_contract
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';

void main() {
  late List<dynamic> tasks;

  setUpAll(() {
    final raw = File('test/fixtures/bot_tasks.json').readAsStringSync();
    final fixture = jsonDecode(raw) as Map<String, dynamic>;
    tasks = fixture['tasks'] as List<dynamic>;
  });

  group('BotTask.fromJson — real Rust bot_tasks payload', () {
    // ── done task: ts fields are unix seconds ─────────────────────────────────
    test('done task: ts fields are unix seconds', () {
      final raw = tasks.firstWhere(
        (t) => (t as Map)['state'] == 'done',
      ) as Map<String, dynamic>;
      final task = BotTask.fromJson(raw);

      expect(task.id, 'task-done');
      expect(task.state, 'done');
      expect(task.ts, greaterThan(1_000_000_000));
      expect(task.ts, lessThan(20_000_000_000));
      expect(task.startedAt, isNotNull);
      expect(task.finishedAt, isNotNull);
      expect(task.result, 'LGTM');
    });

    // ── running task: new fields title/preparing/tz ───────────────────────────
    test('running task: title, preparing, tz fields parse', () {
      // Incident: title/preparing/tz were absent from BotTask; chat showed no
      // agent title and no "Downloading" indicator for large repos.
      final raw = tasks.firstWhere(
        (t) => (t as Map)['state'] == 'running',
      ) as Map<String, dynamic>;
      final task = BotTask.fromJson(raw);

      expect(task.id, 'task-running');
      expect(task.state, 'running');
      expect(
        task.title,
        'Writing unit tests for store',
        reason: 'title field must parse from Rust Task.title',
      );
      expect(
        task.preparing,
        'codeatlasdev/ui',
        reason: 'preparing field must parse from Rust Task.preparing',
      );
      expect(
        task.tz,
        'America/Sao_Paulo',
        reason: 'tz field must parse from Rust Task.tz',
      );
    });

    // ── running task: step.group parses ──────────────────────────────────────
    test('running task: step.group field parses', () {
      // Incident: BotStep.group was absent; chat could not group steps by title.
      final raw = tasks.firstWhere(
        (t) => (t as Map)['state'] == 'running',
      ) as Map<String, dynamic>;
      final task = BotTask.fromJson(raw);

      expect(task.steps, hasLength(2));
      final plain = task.steps.firstWhere((s) => s.group == null);
      expect(plain.text, 'Reading files');
      expect(plain.group, isNull);

      final grouped = task.steps.firstWhere((s) => s.group != null);
      expect(grouped.text, 'Writing tests');
      expect(grouped.group, 'Test suite');
    });

    // ── running task: add_trigger action parses ───────────────────────────────
    test('running task: add_trigger action parses with triggerKind', () {
      // Incident: TaskAction had no triggerKind getter; add_trigger proposals
      // could not be rendered in the chat.
      final raw = tasks.firstWhere(
        (t) => (t as Map)['state'] == 'running',
      ) as Map<String, dynamic>;
      final task = BotTask.fromJson(raw);

      expect(task.action, isNotNull);
      expect(task.action!.kind, 'add_trigger');
      expect(task.action!.reason, 'You asked for a daily review');

      final tk = task.action!.triggerKind;
      expect(tk, isNotNull);
      expect(tk, isA<ScheduleTrigger>());
      expect((tk as ScheduleTrigger).cron, '0 9 * * 1-5');

      expect(task.action!.triggerPrompt, 'Review open PRs');
    });

    // ── BotTask missing optional fields: defaults gracefully ─────────────────
    test('BotTask with no new fields uses null defaults', () {
      final j = <String, dynamic>{
        'id': 'x',
        'ts': 1700000000,
        'source': 'desktop',
        'prompt': 'test',
        'state': 'queued',
      };
      final t = BotTask.fromJson(j);
      expect(t.title, isNull);
      expect(t.preparing, isNull);
      expect(t.tz, isNull);
      expect(t.steps, isEmpty);
      expect(t.action, isNull);
    });
  });
}
