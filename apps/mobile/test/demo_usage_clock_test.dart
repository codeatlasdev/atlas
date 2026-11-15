// Incident: the usage screen's test (and the App Review demo) showed "Today: 0" from 00:00 to 01:00 local.
// The demo seeded "today's" turn one hour before now, and "today" starts at local midnight, so in the first
// hour of the day the seeded turn was yesterday's. The demo's clock is a dependency now: its "today" record is
// inside today at every minute of the day, and the totals the screen computes (`usageTotals`) say so.
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/demo/demo_crew.dart';
import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';

void main() {
  /// The usage totals the Usage screen shows for crewmate-1 when the device clock reads [now].
  UsageTotals totalsAt(DateTime now) {
    final ops = DemoCrewOps(seedDemoCrew(), clock: () => now);
    final records = parseUsage(ops.usage('crewmate-1'));
    return usageTotals(records, now);
  }

  // The seeded turn of "today" is 1800 input + 320 output tokens.
  const seededToday = 1800 + 320;

  test(
    'today shows the seeded turn in the first hour after local midnight',
    () {
      expect(totalsAt(DateTime(2026, 11, 15, 0, 30)).today, seededToday);
    },
  );

  test('today shows the seeded turn at every minute boundary of the day', () {
    for (final (h, m) in [(0, 0), (0, 1), (0, 59), (1, 0), (12, 0), (23, 59)]) {
      expect(
        totalsAt(DateTime(2026, 11, 15, h, m)).today,
        seededToday,
        reason: 'clock at $h:${m.toString().padLeft(2, '0')}',
      );
    }
  });

  test(
    'the older days are still older days (the week and month keep their sums)',
    () {
      final t = totalsAt(DateTime(2026, 11, 15, 0, 30));
      expect(t.today, lessThan(t.week));
      expect(t.week, lessThanOrEqualTo(t.month));
    },
  );
}
