// Pure-logic tests for automations.dart, mirroring automations.test.ts cases.
// No Flutter widgets; just the Dart port of the TS logic.
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/features/crew/automations.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';

void main() {
  // ─── scheduleFromCron ──────────────────────────────────────────────────────

  group('scheduleFromCron', () {
    test('every day (wildcard dow)', () {
      final spec = scheduleFromCron('0 9 * * *');
      expect(spec, isNotNull);
      expect(spec!.days, [0, 1, 2, 3, 4, 5, 6]);
      expect(spec.hour, 9);
      expect(spec.minute, 0);
    });

    test('weekdays range 1-5', () {
      final spec = scheduleFromCron('0 9 * * 1-5');
      expect(spec, isNotNull);
      expect(spec!.days, [1, 2, 3, 4, 5]);
      expect(spec.hour, 9);
      expect(spec.minute, 0);
    });

    test('single day', () {
      final spec = scheduleFromCron('30 18 * * 1');
      expect(spec, isNotNull);
      expect(spec!.days, [1]);
      expect(spec.hour, 18);
      expect(spec.minute, 30);
    });

    test('comma-separated days', () {
      final spec = scheduleFromCron('0 8 * * 1,3,5');
      expect(spec, isNotNull);
      expect(spec!.days, [1, 3, 5]);
      expect(spec.hour, 8);
      expect(spec.minute, 0);
    });

    test('step expression returns null (unsupported)', () {
      expect(scheduleFromCron('0 */3 * * *'), isNull);
    });

    test('non-wildcard dom/month returns null', () {
      expect(scheduleFromCron('0 9 1 * *'), isNull);
    });

    test('wrong field count returns null', () {
      expect(scheduleFromCron('0 9 * *'), isNull);
    });
  });

  // ─── cronFromSchedule → scheduleFromCron round-trips ──────────────────────

  group('cronFromSchedule round-trip', () {
    final cases = [
      (
        'every day',
        ScheduleSpec(days: [0, 1, 2, 3, 4, 5, 6], hour: 9, minute: 0),
      ),
      ('weekdays', ScheduleSpec(days: [1, 2, 3, 4, 5], hour: 9, minute: 0)),
      ('single day Mon', ScheduleSpec(days: [1], hour: 18, minute: 30)),
      (
        'non-contiguous Mon Wed Fri',
        ScheduleSpec(days: [1, 3, 5], hour: 8, minute: 0),
      ),
      ('weekends', ScheduleSpec(days: [0, 6], hour: 10, minute: 0)),
    ];

    for (final (label, spec) in cases) {
      test(label, () {
        final cron = cronFromSchedule(spec);
        final parsed = scheduleFromCron(cron);
        expect(
          parsed,
          isNotNull,
          reason: 'round-trip must produce a valid cron: $cron',
        );
        final sorted = [...spec.days]..sort();
        expect(parsed!.days, sorted);
        expect(parsed.hour, spec.hour);
        expect(parsed.minute, spec.minute);
      });
    }

    test('contiguous range is compact (not comma-separated)', () {
      expect(
        cronFromSchedule(
          ScheduleSpec(days: [1, 2, 3, 4, 5], hour: 9, minute: 0),
        ),
        '0 9 * * 1-5',
      );
    });

    test('all days uses wildcard', () {
      expect(
        cronFromSchedule(
          ScheduleSpec(days: [0, 1, 2, 3, 4, 5, 6], hour: 9, minute: 0),
        ),
        '0 9 * * *',
      );
    });
  });

  // ─── describeTrigger ──────────────────────────────────────────────────────

  group('describeTrigger', () {
    test('schedule every day', () {
      expect(
        describeTrigger(const ScheduleTrigger(cron: '0 9 * * *')),
        'Every day at 09:00',
      );
    });

    test('schedule weekdays', () {
      expect(
        describeTrigger(const ScheduleTrigger(cron: '0 9 * * 1-5')),
        'Weekdays at 09:00',
      );
    });

    test('schedule specific days', () {
      expect(
        describeTrigger(const ScheduleTrigger(cron: '30 18 * * 1,3,5')),
        'Mon, Wed, Fri at 18:30',
      );
    });

    test('schedule with tz appended', () {
      expect(
        describeTrigger(
          const ScheduleTrigger(cron: '0 9 * * 1-5'),
          tz: 'America/Sao_Paulo',
        ),
        'Weekdays at 09:00 (America/Sao_Paulo)',
      );
    });

    test('schedule custom cron fallback', () {
      expect(
        describeTrigger(const ScheduleTrigger(cron: '0 */3 * * *')),
        'Custom: 0 */3 * * *',
      );
    });

    test('interval exact minutes', () {
      expect(
        describeTrigger(const IntervalTrigger(everySecs: 1800)),
        'Every 30 minutes',
      );
    });

    test('interval 1 hour (singular)', () {
      expect(
        describeTrigger(const IntervalTrigger(everySecs: 3600)),
        'Every 1 hour',
      );
    });

    test('interval 2 hours', () {
      expect(
        describeTrigger(const IntervalTrigger(everySecs: 7200)),
        'Every 2 hours',
      );
    });

    test('interval fractional hours shows h m', () {
      expect(
        describeTrigger(const IntervalTrigger(everySecs: 5400)),
        'Every 1h 30m',
      );
    });

    test('webhook', () {
      expect(describeTrigger(const WebhookTrigger(token: 'abc123')), 'Webhook');
    });

    test('github pr_opened', () {
      expect(
        describeTrigger(
          const GithubTrigger(repo: 'owner/repo', events: ['pr_opened']),
        ),
        'When a PR is opened in owner/repo',
      );
    });

    test('github ci_failed', () {
      expect(
        describeTrigger(
          const GithubTrigger(repo: 'owner/repo', events: ['ci_failed']),
        ),
        'When CI fails in owner/repo',
      );
    });

    test('github multiple events joined with or', () {
      expect(
        describeTrigger(
          const GithubTrigger(
            repo: 'owner/repo',
            events: ['pr_opened', 'ci_failed'],
          ),
        ),
        'When a PR is opened or CI fails in owner/repo',
      );
    });

    test('github three events: last separated by or', () {
      expect(
        describeTrigger(
          const GithubTrigger(
            repo: 'owner/repo',
            events: ['pr_opened', 'ci_failed', 'release'],
          ),
        ),
        'When a PR is opened, CI fails or a release is published in owner/repo',
      );
    });

    test('file with glob', () {
      expect(
        describeTrigger(const FileTrigger(path: '/src', glob: '**/*.ts')),
        'When **/*.ts changes',
      );
    });

    test('file without glob falls back to path', () {
      expect(
        describeTrigger(const FileTrigger(path: '/src/index.ts')),
        'When /src/index.ts changes',
      );
    });

    test('crewmate done', () {
      expect(
        describeTrigger(
          const CrewmateTrigger(fromCrewmate: 'Reviewer', on: 'done'),
        ),
        'When Reviewer finishes',
      );
    });

    test('crewmate failed', () {
      expect(
        describeTrigger(
          const CrewmateTrigger(fromCrewmate: 'Reviewer', on: 'failed'),
        ),
        'When Reviewer fails',
      );
    });

    test('service failed', () {
      expect(
        describeTrigger(
          const ServiceTrigger(
            projectDir: '/app',
            service: 'web',
            on: 'failed',
          ),
        ),
        'When web fails',
      );
    });

    test('service recovered', () {
      expect(
        describeTrigger(
          const ServiceTrigger(
            projectDir: '/app',
            service: 'web',
            on: 'recovered',
          ),
        ),
        'When web recovers',
      );
    });
  });

  // ─── describeNext ─────────────────────────────────────────────────────────

  group('describeNext', () {
    // Fixed reference: 2024-06-10 Monday 08:00 UTC
    final now = DateTime.utc(2024, 6, 10, 8, 0, 0);

    test("same calendar day → 'today at …'", () {
      final fire = DateTime.utc(2024, 6, 10, 9, 0, 0);
      final result = describeNext(fire.millisecondsSinceEpoch ~/ 1000, now);
      expect(result, startsWith('today at '));
    });

    test("next calendar day → 'tomorrow at …'", () {
      final fire = DateTime.utc(2024, 6, 11, 9, 0, 0);
      final result = describeNext(fire.millisecondsSinceEpoch ~/ 1000, now);
      expect(result, startsWith('tomorrow at '));
    });

    test('further out → weekday abbreviation prefix', () {
      // 2024-06-12 is Wednesday
      final fire = DateTime.utc(2024, 6, 12, 9, 0, 0);
      final result = describeNext(fire.millisecondsSinceEpoch ~/ 1000, now);
      expect(result, isNot(startsWith('today')));
      expect(result, isNot(startsWith('tomorrow')));
      expect(result.length, greaterThan(3));
    });
  });
}
