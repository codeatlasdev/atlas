/// Pure utilities for crewmate automations: cron ↔ builder conversion,
/// human-readable trigger descriptions, and next-fire descriptions.
/// No side-effects; safe to import in tests.
library;

import 'package:atlas_mobile/core/protocol/crew_models.dart';

// ─── Schedule spec ────────────────────────────────────────────────────────────

/// Day-of-week indices: 0 = Sun … 6 = Sat (matches cron DOW).
class ScheduleSpec {
  const ScheduleSpec({
    required this.days,
    required this.hour,
    required this.minute,
  });
  final List<int> days;
  final int hour;
  final int minute;
}

const _dayNames = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

// ─── cron ↔ builder ──────────────────────────────────────────────────────────

/// Parse a "m h * * dow" cron into a [ScheduleSpec], or null for unsupported
/// patterns (non-standard fields, step expressions, multiple ranges, etc.).
ScheduleSpec? scheduleFromCron(String cron) {
  final parts = cron.trim().split(RegExp(r'\s+'));
  if (parts.length != 5) return null;
  final min = parts[0];
  final hour = parts[1];
  final dom = parts[2];
  final month = parts[3];
  final dow = parts[4];

  if (dom != '*' || month != '*') return null;

  // minute + hour must be simple integers
  if (!RegExp(r'^\d+$').hasMatch(min) || !RegExp(r'^\d+$').hasMatch(hour)) {
    return null;
  }
  final minuteN = int.parse(min);
  final hourN = int.parse(hour);
  if (hourN < 0 || hourN > 23 || minuteN < 0 || minuteN > 59) return null;

  List<int> days;
  if (dow == '*') {
    days = [0, 1, 2, 3, 4, 5, 6];
  } else if (RegExp(r'^\d+$').hasMatch(dow)) {
    final d = int.parse(dow);
    if (d < 0 || d > 6) return null;
    days = [d];
  } else if (RegExp(r'^\d+-\d+$').hasMatch(dow)) {
    final ab = dow.split('-').map(int.parse).toList();
    final a = ab[0];
    final b = ab[1];
    if (a > b || a < 0 || b > 6) return null;
    days = List.generate(b - a + 1, (i) => a + i);
  } else if (RegExp(r'^[\d,]+$').hasMatch(dow)) {
    final ds = dow.split(',').map(int.parse).toList();
    if (ds.any((d) => d < 0 || d > 6)) return null;
    days = ds;
  } else {
    return null;
  }

  return ScheduleSpec(days: days, hour: hourN, minute: minuteN);
}

/// Produce a canonical "m h * * dow" cron string from a [ScheduleSpec].
String cronFromSchedule(ScheduleSpec spec) {
  final sorted = [...spec.days]..sort();
  String dow;
  if (sorted.length == 7) {
    dow = '*';
  } else if (sorted.length == 1) {
    dow = '${sorted[0]}';
  } else {
    // Prefer compact range when contiguous
    var contiguous = true;
    for (var i = 1; i < sorted.length; i++) {
      if (sorted[i] != sorted[i - 1] + 1) {
        contiguous = false;
        break;
      }
    }
    dow = contiguous ? '${sorted.first}-${sorted.last}' : sorted.join(',');
  }
  return '${spec.minute} ${spec.hour} * * $dow';
}

// ─── human-readable descriptions ─────────────────────────────────────────────

const _githubEventPhrase = <String, String>{
  'pr_opened': 'a PR is opened',
  'pr_review_requested': 'your review is requested',
  'issue_opened': 'an issue is opened',
  'ci_failed': 'CI fails',
  'release': 'a release is published',
};

/// Produce a human-readable one-liner for a [TriggerKind].
/// Pass [tz] to append the timezone to schedule descriptions.
String describeTrigger(TriggerKind kind, {String? tz}) {
  return switch (kind) {
    ScheduleTrigger(:final cron) => () {
      final spec = scheduleFromCron(cron);
      if (spec != null) {
        final sorted = [...spec.days]..sort();
        final String dayLabel;
        if (sorted.length == 7) {
          dayLabel = 'Every day';
        } else if (sorted.length == 5 && sorted.join(',') == '1,2,3,4,5') {
          dayLabel = 'Weekdays';
        } else if (sorted.length == 2 && sorted.join(',') == '0,6') {
          dayLabel = 'Weekends';
        } else {
          dayLabel = sorted.map((d) => _dayNames[d]).join(', ');
        }
        final timeLabel =
            '${spec.hour.toString().padLeft(2, '0')}:${spec.minute.toString().padLeft(2, '0')}';
        final tzNote = tz != null ? ' ($tz)' : '';
        return '$dayLabel at $timeLabel$tzNote';
      }
      return 'Custom: $cron';
    }(),
    IntervalTrigger(:final everySecs) => () {
      if (everySecs < 60) return 'Every ${everySecs}s';
      final m = everySecs ~/ 60;
      if (everySecs < 3600 && everySecs % 60 == 0) {
        return 'Every $m ${m != 1 ? 'minutes' : 'minute'}';
      }
      final h = everySecs ~/ 3600;
      if (everySecs % 3600 == 0) {
        return 'Every $h ${h != 1 ? 'hours' : 'hour'}';
      }
      return 'Every ${everySecs ~/ 3600}h ${(everySecs % 3600) ~/ 60}m';
    }(),
    WebhookTrigger() => 'Webhook',
    GithubTrigger(:final events, :final repo) => () {
      final phrases = events
          .map((e) => _githubEventPhrase[e] ?? e.replaceAll('_', ' '))
          .toList();
      final String list;
      if (phrases.isEmpty) {
        list = '';
      } else if (phrases.length == 1) {
        list = phrases[0];
      } else {
        list =
            '${phrases.sublist(0, phrases.length - 1).join(', ')} or ${phrases.last}';
      }
      return 'When $list in $repo';
    }(),
    FileTrigger(:final glob, :final path) => 'When ${glob ?? path} changes',
    CrewmateTrigger(:final fromCrewmate, :final on) =>
      'When $fromCrewmate ${on == 'done' ? 'finishes' : 'fails'}',
    ServiceTrigger(:final service, :final on) =>
      'When $service ${on == 'failed' ? 'fails' : 'recovers'}',
  };
}

/// Describe when the next fire will occur relative to [now].
/// [nextFire] is unix seconds; [now] is a [DateTime].
String describeNext(int nextFire, DateTime now) {
  final fire = DateTime.fromMillisecondsSinceEpoch(nextFire * 1000);
  final timeStr =
      '${fire.hour.toString().padLeft(2, '0')}:${fire.minute.toString().padLeft(2, '0')}';

  if (fire.year == now.year && fire.month == now.month && fire.day == now.day) {
    return 'today at $timeStr';
  }
  final tomorrow = now.add(const Duration(days: 1));
  if (fire.year == tomorrow.year &&
      fire.month == tomorrow.month &&
      fire.day == tomorrow.day) {
    return 'tomorrow at $timeStr';
  }
  // Further out: weekday abbreviation
  const weekdays = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
  final wd = weekdays[fire.weekday - 1];
  return '$wd $timeStr';
}
