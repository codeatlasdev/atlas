// `LatestOnly` is the rule a polling screen lives by on a slow link: the newest question's reply wins,
// and a timer never queues a second request behind one that is still in flight.
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/latest_only.dart';

void main() {
  test('only the newest ticket is current', () {
    final l = LatestOnly();
    final a = l.begin();
    expect(a.isCurrent, isTrue);
    final b = l.begin();
    expect((a.isCurrent, b.isCurrent), (false, true));
  });

  test(
    'tryBegin refuses while a ticket is in flight and resumes after done',
    () {
      final l = LatestOnly();
      final a = l.tryBegin()!;
      expect(l.tryBegin(), isNull, reason: 'one poll at a time');
      a.done();
      final b = l.tryBegin();
      expect(b, isNotNull);
      expect(b!.isCurrent, isTrue);
    },
  );

  test('done is idempotent: a double done does not free a second slot', () {
    final l = LatestOnly();
    final a = l.tryBegin()!;
    a.done();
    a.done();
    final b = l.tryBegin()!;
    expect(
      l.tryBegin(),
      isNull,
      reason: 'b is in flight; a double done must not have gone negative',
    );
    b.done();
  });

  test(
    'a user-driven begin runs even while a poll is in flight and supersedes it',
    () {
      final l = LatestOnly();
      final poll = l.tryBegin()!;
      final search = l.begin();
      expect((poll.isCurrent, search.isCurrent), (false, true));
      // The superseded poll finishing must not free the slot while the search is still open.
      poll.done();
      expect(l.tryBegin(), isNull);
      search.done();
      expect(l.tryBegin(), isNotNull);
    },
  );

  test('close makes everything stale and refuses new polls', () {
    final l = LatestOnly();
    final a = l.begin();
    l.close();
    expect(a.isCurrent, isFalse);
    a.done();
    expect(l.tryBegin(), isNull);
  });
}
