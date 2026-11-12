// Guards 2026-11-12 (the Crew list on the phone): every row said "20748d" because `last.ts`
// (seconds) was read as milliseconds, and previews showed raw markdown.
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/features/bots/crew_text.dart';

void main() {
  final now = DateTime(2026, 11, 12, 13, 35);
  int s(DateTime d) => d.millisecondsSinceEpoch ~/ 1000;

  test('last.ts is seconds since the epoch', () {
    expect(crewTimeAgo(s(now.subtract(const Duration(minutes: 5))), now), '5m');
    expect(crewTimeAgo(s(now.subtract(const Duration(hours: 3))), now), '3h');
    expect(
      crewTimeAgo(s(now.subtract(const Duration(days: 1, hours: 2))), now),
      'yesterday',
    );
    expect(
      crewTimeAgo(s(now.subtract(const Duration(seconds: 20))), now),
      'now',
    );
  });

  test('older than a week is a date, with the year only when it differs', () {
    expect(crewTimeAgo(s(DateTime(2026, 6, 29, 10)), now), '29 Jun');
    expect(crewTimeAgo(s(DateTime(2025, 12, 1)), now), '1 Dec 2025');
    expect(crewTimeAgo(0, now), '');
  });

  test('a preview is one plain line without markdown or urls', () {
    expect(
      crewPreview('- **Estado:** #14 (https://github.com/a/b/pull/14) aberto'),
      'Estado: #14 aberto',
    );
    expect(
      crewPreview('Combinado, vou aguardar.\n\n1. Rodo o `CI`'),
      'Combinado, vou aguardar. Rodo o CI',
    );
    expect(
      crewPreview('Veja [o PR](https://x.y/1) e _isso_'),
      'Veja o PR e isso',
    );
    expect(crewPreview('## Done\n> quote'), 'Done quote');
  });

  test('role tag', () {
    expect(crewRoleTag('Reviewer: checks every PR'), 'Reviewer');
    expect(crewRoleTag('DevOps keeps the ship'), 'DevOps');
    expect(crewRoleTag('keeps the ship running all night long'), '');
    expect(crewRoleTag(''), '');
  });
  // The chat read task times (seconds) as milliseconds too: every bubble said 1970.
  test('chat time is the clock today, with the day otherwise', () {
    expect(crewChatTime(s(DateTime(2026, 11, 12, 9, 5)), now), '09:05');
    expect(
      crewChatTime(s(DateTime(2026, 11, 11, 22, 40)), now),
      'Yesterday 22:40',
    );
    expect(crewChatTime(s(DateTime(2026, 11, 9, 8, 0)), now), 'Mon 08:00');
    expect(crewChatTime(s(DateTime(2026, 6, 29, 14, 3)), now), '29 Jun, 14:03');
  });

  test('elapsed counts from started_at in seconds', () {
    expect(
      crewElapsed(s(now.subtract(const Duration(seconds: 42))), now),
      '42s',
    );
    expect(crewElapsed(s(now.subtract(const Duration(minutes: 7))), now), '7m');
    expect(
      crewElapsed(s(now.subtract(const Duration(minutes: 75))), now),
      '1h 15m',
    );
  });

  test('a thread is split by day, the clock stays in the bubble', () {
    expect(
      crewDayLabel(s(DateTime(2026, 11, 12, 8)), now),
      'Today, 12 November',
    );
    expect(
      crewDayLabel(s(DateTime(2026, 11, 11, 23, 59)), now),
      'Yesterday, 11 November',
    );
    expect(crewDayLabel(s(DateTime(2026, 9, 3)), now), '3 September');
    expect(crewDayLabel(s(DateTime(2025, 12, 31)), now), '31 December 2025');
    expect(crewClock(s(DateTime(2026, 11, 12, 7, 4))), '07:04');
    expect(
      crewDayOf(s(DateTime(2026, 11, 12, 0, 1))),
      isNot(crewDayOf(s(DateTime(2026, 11, 11, 23, 59)))),
    );
  });
}
