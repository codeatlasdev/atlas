// Contract: CrewmateEntry parses triggers from the real Rust crewmate_list fixture.
// Incident: triggers field was absent from CrewmateEntry; routines used instead.
// Regenerate fixture: UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_list_json_contract
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';

void main() {
  late List<dynamic> crewmates;

  setUpAll(() {
    final raw = File('test/fixtures/crewmate_list.json').readAsStringSync();
    final fixture = jsonDecode(raw) as Map<String, dynamic>;
    crewmates = fixture['crewmates'] as List<dynamic>;
  });

  group('CrewmateEntry.fromJson — real Rust crewmate_list payload', () {
    // ── working crewmate: running_task is an OBJECT {id, pane} ────────────────────
    test('parses working crewmate: running_task is CrewmateRunningTask, not null/String', () {
      final raw = crewmates.firstWhere(
        (b) => (b as Map)['state'] == 'working',
      ) as Map<String, dynamic>;
      final entry = CrewmateEntry.fromJson(raw);

      expect(entry.state, 'working');
      expect(entry.agent, 'claude');
      expect(entry.queued, 0);

      // THE BUG: runningTask was String? and `j['running_task'] as String?`
      // returned null for an object, crashing callers that used the pane key.
      // After the fix, runningTask must be a CrewmateRunningTask with id and pane.
      expect(
        entry.runningTask,
        isNotNull,
        reason: 'runningTask must parse the {id,pane} object — not return null',
      );
      expect(entry.runningTask!.id, 'task-working');
      expect(entry.runningTask!.pane, 'crew:crewmate-working:task-working');
    });

    // ── working crewmate: last field present ──────────────────────────────────────
    test('parses last field on working crewmate', () {
      final raw = crewmates.firstWhere(
        (b) => (b as Map)['state'] == 'working',
      ) as Map<String, dynamic>;
      final entry = CrewmateEntry.fromJson(raw);

      expect(entry.last, isNotNull);
      expect(entry.last!.text, isNotEmpty);
      expect(entry.last!.ts, isPositive);
    });

    // ── idle crewmate with done task: last present, no running_task ────────────────
    test('parses idle crewmate: last present, running_task absent', () {
      final raw = crewmates.firstWhere(
        (b) => (b as Map)['id'] == 'crewmate-done',
      ) as Map<String, dynamic>;
      final entry = CrewmateEntry.fromJson(raw);

      expect(entry.id, 'crewmate-done');
      expect(entry.state, 'idle');
      expect(entry.last, isNotNull);
      expect(entry.last!.text, 'nightly done');
      expect(entry.last!.ts, 1700000000);
      expect(entry.runningTask, isNull);
      expect(entry.queued, 0);
      // No engine key → approvalSupported defaults to true.
      expect(
        entry.approvalSupported,
        isTrue,
        reason: 'claude crewmate without engine key defaults approvalSupported to true',
      );
    });

    // ── opencode crewmate: approval_supported:false ───────────────────────────────
    test('parses opencode crewmate: approvalSupported is false', () {
      final raw = crewmates.firstWhere(
        (b) => (b as Map)['id'] == 'crewmate-opencode',
      ) as Map<String, dynamic>;
      final entry = CrewmateEntry.fromJson(raw);

      expect(entry.id, 'crewmate-opencode');
      expect(entry.agent, 'opencode');
      expect(entry.state, 'idle');
      // Real Rust producer emits approval_supported:false for opencode crewmates.
      expect(
        entry.approvalSupported,
        isFalse,
        reason: 'opencode crewmate must have approvalSupported=false from the real producer',
      );
      expect(entry.runningTask, isNull);
    });

    // ── crewmate-done: triggers with schedule (tz, next_fire) and interval ────────
    test(
      'parses triggers on crewmate-done: schedule trigger has tz and next_fire',
      () {
        final raw = crewmates.firstWhere(
          (b) => (b as Map)['id'] == 'crewmate-done',
        ) as Map<String, dynamic>;
        final entry = CrewmateEntry.fromJson(raw);

        expect(
          entry.triggers,
          hasLength(2),
          reason:
              'crewmate-done must have 2 triggers from crewmate_triggers_set',
        );

        final sched = entry.triggers.firstWhere(
          (t) => t.kind is ScheduleTrigger,
        );
        expect(sched.id, 't-sched');
        expect(sched.enabled, isTrue);
        expect(sched.prompt, 'Review open PRs');
        expect(sched.tz, 'America/Sao_Paulo');
        expect(sched.nextFire, isNotNull);
        expect(sched.nextFire, isPositive);
        final schedKind = sched.kind as ScheduleTrigger;
        expect(schedKind.cron, '0 9 * * 1-5');

        final interval = entry.triggers.firstWhere(
          (t) => t.kind is IntervalTrigger,
        );
        expect(interval.id, 't-interval');
        expect(interval.enabled, isFalse);
        expect(interval.prompt, 'Check deployments');
        final intervalKind = interval.kind as IntervalTrigger;
        expect(intervalKind.everySecs, 3600);
      },
    );

    // ── working crewmate: triggers is empty list, not null ─────────────────────────
    test('parses working crewmate: triggers is empty list', () {
      final raw = crewmates.firstWhere(
        (b) => (b as Map)['state'] == 'working',
      ) as Map<String, dynamic>;
      final entry = CrewmateEntry.fromJson(raw);
      expect(
        entry.triggers,
        isEmpty,
        reason: 'working crewmate has no triggers in fixture',
      );
    });
  });

  group('Trigger.fromJson round-trip', () {
    // ── all TriggerKind variants parse correctly ──────────────────────────────
    test('schedule trigger parses and round-trips', () {
      final j = <String, dynamic>{
        'id': 'x',
        'kind': {'type': 'schedule', 'cron': '0 9 * * *'},
        'enabled': true,
        'prompt': 'p',
        'tz': 'UTC',
        'next_fire': 9999,
      };
      final t = Trigger.fromJson(j);
      expect(t.kind, isA<ScheduleTrigger>());
      expect((t.kind as ScheduleTrigger).cron, '0 9 * * *');
      expect(t.tz, 'UTC');
      expect(t.nextFire, 9999);
      expect(t.toJson()['kind'], {'type': 'schedule', 'cron': '0 9 * * *'});
    });

    test('interval trigger parses', () {
      final j = <String, dynamic>{
        'id': 'y',
        'kind': {'type': 'interval', 'every_secs': 3600},
        'enabled': false,
        'prompt': 'p',
      };
      final t = Trigger.fromJson(j);
      expect(t.kind, isA<IntervalTrigger>());
      expect((t.kind as IntervalTrigger).everySecs, 3600);
      expect(t.enabled, isFalse);
    });

    test('github trigger parses', () {
      final j = <String, dynamic>{
        'id': 'g',
        'kind': {
          'type': 'github',
          'repo': 'owner/repo',
          'events': ['pr_opened', 'ci_failed'],
        },
        'enabled': true,
        'prompt': 'p',
      };
      final t = Trigger.fromJson(j);
      expect(t.kind, isA<GithubTrigger>());
      final k = t.kind as GithubTrigger;
      expect(k.repo, 'owner/repo');
      expect(k.events, ['pr_opened', 'ci_failed']);
    });

    test('file trigger parses with optional glob', () {
      final j = <String, dynamic>{
        'id': 'f',
        'kind': {'type': 'file', 'path': '/src', 'glob': '**/*.ts'},
        'enabled': true,
        'prompt': 'p',
      };
      final t = Trigger.fromJson(j);
      expect(t.kind, isA<FileTrigger>());
      final k = t.kind as FileTrigger;
      expect(k.path, '/src');
      expect(k.glob, '**/*.ts');
    });

    test('crewmate trigger parses', () {
      final j = <String, dynamic>{
        'id': 'b',
        'kind': {'type': 'crewmate', 'from_crewmate': 'Reviewer', 'on': 'done'},
        'enabled': true,
        'prompt': 'p',
      };
      final t = Trigger.fromJson(j);
      expect(t.kind, isA<CrewmateTrigger>());
      final k = t.kind as CrewmateTrigger;
      expect(k.fromCrewmate, 'Reviewer');
      expect(k.on, 'done');
    });

    test('service trigger parses', () {
      final j = <String, dynamic>{
        'id': 's',
        'kind': {
          'type': 'service',
          'project_dir': '/app',
          'service': 'api',
          'on': 'failed',
        },
        'enabled': true,
        'prompt': 'p',
      };
      final t = Trigger.fromJson(j);
      expect(t.kind, isA<ServiceTrigger>());
      final k = t.kind as ServiceTrigger;
      expect(k.service, 'api');
      expect(k.on, 'failed');
    });
  });
}
