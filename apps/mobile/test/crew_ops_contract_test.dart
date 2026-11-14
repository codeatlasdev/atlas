// Contract: the phone's crew operations models (audit, usage, engine limits, crew settings, routines)
// parse the REAL replies of hostd, written by its serializer, never by hand:
//   UPDATE_FIXTURES=1 cargo test -p atlas-host --lib -- mobile_payloads crewmate_list_json_contract
// Incident (gap #5): the desktop had these screens, the phone had none, and the commands had no Dart caller.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';

Map<String, dynamic> _fixture(String name) =>
    jsonDecode(File('test/fixtures/$name').readAsStringSync())
        as Map<String, dynamic>;

void main() {
  group('audit — real crewmate_audit and crew_audit replies', () {
    test('a crewmate audit parses every kind, newest first', () {
      final entries = parseAudit(_fixture('crewmate_audit.json'));
      expect(entries.map((e) => e.kind).toSet(), {
        'task_queued',
        'task_started',
        'tool_call',
        'task_finished',
        'config_changed',
        'secret_set',
      });
      final seqs = entries.map((e) => e.seq).toList();
      expect(seqs, [...seqs]..sort((a, b) => b.compareTo(a)));
    });

    test('the crew-wide audit parses its own kinds', () {
      final entries = parseAudit(_fixture('crew_audit.json'));
      expect(entries.map((e) => e.kind), [
        'resumed',
        'paused',
        'crewmate_created',
      ]);
    });

    test('summaries read like the desktop Audit section', () {
      final by = {
        for (final e in parseAudit(_fixture('crewmate_audit.json'))) e.kind: e,
      };
      expect(by['tool_call']!.summary, 'Bash: cargo test -p atlas-host');
      expect(by['task_queued']!.summary, startsWith('Task queued (test): '));
      expect(by['task_finished']!.summary, 'Task finished: done');
      expect(by['config_changed']!.summary, 'Changed approval: "auto" → "ask"');
      expect(by['secret_set']!.summary, 'Set secret GITHUB_TOKEN');
      final crew = {
        for (final e in parseAudit(_fixture('crew_audit.json'))) e.kind: e,
      };
      expect(crew['paused']!.summary, 'Crew paused');
      expect(crew['resumed']!.summary, 'Crew resumed');
      expect(crew['crewmate_created']!.summary, 'Crewmate Fixture created');
    });

    test('a secret value never reaches the phone, only its name', () {
      final raw = File('test/fixtures/crewmate_audit.json').readAsStringSync();
      expect(raw, isNot(contains('ghp_')));
    });

    test(
      'filters group kinds like the desktop: tools, approvals, tasks, settings',
      () {
        final entries = parseAudit(_fixture('crewmate_audit.json'));
        Set<String> kinds(AuditGroup? g) =>
            filterAudit(entries, g).map((e) => e.kind).toSet();
        expect(kinds(null), hasLength(6));
        expect(kinds(AuditGroup.tools), {'tool_call'});
        expect(kinds(AuditGroup.tasks), {
          'task_queued',
          'task_started',
          'task_finished',
        });
        expect(kinds(AuditGroup.config), {'config_changed', 'secret_set'});
        expect(kinds(AuditGroup.approvals), isEmpty);
      },
    );

    test('an approval entry is read as who answered and what', () {
      final e = AuditEntry.fromJson({
        'seq': 8,
        'ts': 1,
        'kind': 'approval_decision',
        'decision': 'deny',
        'tool_name': 'Bash',
        'by': 'phone:abcdef0123456789',
      });
      expect(e.summary, 'Denied Bash (answered by phone abcdef01…)');
      final timeout = AuditEntry.fromJson({
        'seq': 9,
        'ts': 1,
        'kind': 'approval_decision',
        'decision': 'timeout',
        'tool_name': 'Bash',
        'by': 'none',
      });
      expect(timeout.summary, 'Timed out Bash (nobody answered)');
    });

    test(
      'a reply from an older server, or a malformed row, gives no entry',
      () {
        expect(parseAudit(const {}), isEmpty);
        expect(
          parseAudit({
            'entries': [
              {'seq': 1, 'ts': 2, 'kind': 'tool_call'},
              {'kind': 'tool_call'},
              'junk',
              null,
            ],
          }),
          hasLength(1),
        );
      },
    );

    test(
      'an unknown kind from a newer server still lists, under its own name',
      () {
        final e = AuditEntry.fromJson({'seq': 1, 'ts': 2, 'kind': 'new_thing'});
        expect(e.summary, 'new_thing');
        expect(auditKindLabel('new_thing'), 'new_thing');
        expect(auditKindLabel('tool_call'), 'Tool call');
      },
    );

    test(
      'a long tool input is clipped in the list but kept whole for copy',
      () {
        final long = 'z' * 300;
        final e = AuditEntry.fromJson({
          'seq': 1,
          'ts': 2,
          'kind': 'tool_call',
          'tool_name': 'Bash',
          'tool_input': {'command': long},
        });
        expect(e.summary.length, lessThan(140));
        expect(e.summary, endsWith('…'));
        expect(e.copyText, contains(long));
      },
    );
  });

  group('usage — real crewmate_usage reply', () {
    test('records parse with tokens, cost and engine', () {
      final records = parseUsage(_fixture('crewmate_usage.json'));
      expect(records, hasLength(2));
      expect(records.first.inputTokens, 1200);
      expect(records.first.outputTokens, 340);
      expect(records.first.cacheRead, 9000);
      expect(records.first.costUsd, closeTo(0.0123, 1e-9));
      expect(records.first.engine, 'claude');
      expect(records.first.model, 'claude-sonnet-4');
      expect(records.first.ts, 1700000000);
    });

    test(
      'totals count input + output tokens in today / 7 d / 30 d windows',
      () {
        final records = parseUsage(_fixture('crewmate_usage.json'));
        final at = DateTime.fromMillisecondsSinceEpoch(1700000200 * 1000);
        final t = usageTotals(records, at);
        expect(t.today, 1200 + 340 + 800 + 90);
        expect(t.week, t.today);
        expect(t.month, t.today);
        expect(t.cost, closeTo(0.0246, 1e-9));
        // Ten days later: out of the week, still in the month.
        final later = at.add(const Duration(days: 10));
        final tl = usageTotals(records, later);
        expect(tl.today, 0);
        expect(tl.week, 0);
        expect(tl.month, 2430);
        // 31 days later: out of every window.
        expect(usageTotals(records, at.add(const Duration(days: 31))).month, 0);
      },
    );

    test('the sparkline has one bucket per day, oldest first, today last', () {
      final records = parseUsage(_fixture('crewmate_usage.json'));
      final at = DateTime.fromMillisecondsSinceEpoch(1700000200 * 1000);
      final b = usageBuckets(records, at);
      expect(b, hasLength(30));
      expect(b.last, 2430);
      expect(b.take(29).every((v) => v == 0), isTrue);
    });

    test(
      'a record without tokens (an engine that reports none) counts as zero',
      () {
        final r = UsageRecord.fromJson({'ts': 5, 'engine': 'omp'});
        expect(r.inputTokens, isNull);
        expect(
          usageTotals([r], DateTime.fromMillisecondsSinceEpoch(5000)).today,
          0,
        );
      },
    );

    test('tokens format compactly', () {
      expect(fmtTokens(950), '950');
      expect(fmtTokens(1234), '1.2k');
      expect(fmtTokens(2_500_000), '2.5M');
    });

    test('an empty or old reply gives no records', () {
      expect(parseUsage(const {}), isEmpty);
      expect(parseUsage({'records': []}), isEmpty);
    });
  });

  group('engine limits — real engine_limits reply', () {
    test('no limits exposed: the server message is kept, no engine rows', () {
      final l = EngineLimits.fromJson(_fixture('engine_limits.json'));
      expect(l.limits, isEmpty);
      expect(l.message, contains('no machine-readable quota'));
    });

    test('limits an engine does report are listed by engine, as text', () {
      final l = EngineLimits.fromJson({
        'message': 'm',
        'limits': {
          'claude': {'remaining': 12, 'resets': 'tomorrow'},
        },
      });
      expect(l.limits.keys, ['claude']);
      expect(l.limits['claude'], contains('remaining'));
    });
  });

  group('crew settings — real crew_settings_get reply', () {
    test('the default: short replies on, no model', () {
      final s = CrewSettings.fromJson(_fixture('crew_settings.json'));
      expect(s.shortReplies, isTrue);
      expect(s.shortRepliesModel, isNull);
    });

    test('a saved model and short replies off', () {
      final s = CrewSettings.fromJson(_fixture('crew_settings_custom.json'));
      expect(s.shortReplies, isFalse);
      expect(s.shortRepliesModel, 'haiku');
    });

    test('the request body clears the model with null, like the desktop', () {
      expect(
        const CrewSettings(shortReplies: true, shortRepliesModel: '').toArgs(),
        {'short_replies': true, 'short_replies_model': null},
      );
      expect(
        const CrewSettings(
          shortReplies: false,
          shortRepliesModel: 'haiku',
        ).toArgs(),
        {'short_replies': false, 'short_replies_model': 'haiku'},
      );
    });

    test('an old server reply without the field keeps the default on', () {
      expect(CrewSettings.fromJson(const {}).shortReplies, isTrue);
    });
  });

  group('routines — real crewmate_list reply', () {
    test('a crewmate lists its routines; one without lists none', () {
      final list = parseCrewList(_fixture('crewmate_list.json'), 'x');
      final nightly = list.crewmates.firstWhere((c) => c.id == 'crewmate-done');
      expect(nightly.routines, hasLength(1));
      final r = nightly.routines.single;
      expect(r.id, 'r-nightly');
      expect(r.cron, '30 2 * * *');
      expect(r.prompt, 'Run the nightly checks');
      expect(r.enabled, isTrue);
      expect(r.lastRun, isNull);
      expect(
        list.crewmates.firstWhere((c) => c.id == 'crewmate-working').routines,
        isEmpty,
      );
    });

    test('the request carries the whole list, with the keys hostd reads', () {
      final r = Routine.fromJson({
        'id': 'r',
        'cron': '0 9 * * 1-5',
        'prompt': 'p',
        'enabled': false,
        'last_run': 1700000000,
      });
      expect(r.toJson(), {
        'id': 'r',
        'cron': '0 9 * * 1-5',
        'prompt': 'p',
        'enabled': false,
        'last_run': 1700000000,
      });
    });

    test('a new routine has no last run and is enabled', () {
      final r = Routine.create(id: 'r1', cron: '* * * * *', prompt: 'go');
      expect(r.toJson()['last_run'], isNull);
      expect(r.toJson()['enabled'], isTrue);
    });

    test('routine edits: add, change, toggle, delete keep the others', () {
      const a = Routine(id: 'a', cron: '0 1 * * *', prompt: 'a', enabled: true);
      const b = Routine(id: 'b', cron: '0 2 * * *', prompt: 'b', enabled: true);
      expect(upsertRoutine([a], b).map((r) => r.id), ['a', 'b']);
      final changed = upsertRoutine([a, b], a.copyWith(prompt: 'new'));
      expect(changed.map((r) => r.prompt), ['new', 'b']);
      expect(removeRoutine([a, b], 'a').map((r) => r.id), ['b']);
      expect(
        upsertRoutine([a], a.copyWith(enabled: false)).single.enabled,
        isFalse,
      );
    });

    test(
      'cron validity: five fields of cron characters (hostd has the last word)',
      () {
        expect(looksLikeCron('30 2 * * *'), isTrue);
        expect(looksLikeCron('*/5 * * * 1-5'), isTrue);
        expect(looksLikeCron('not a cron'), isFalse);
        expect(looksLikeCron('* * * *'), isFalse);
        expect(looksLikeCron(''), isFalse);
      },
    );
  });
}
