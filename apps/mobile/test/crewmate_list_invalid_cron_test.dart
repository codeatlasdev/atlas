import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';

// Contract: a cron hostd's parser refuses (saved under an older parser) arrives as `invalid` on the trigger and
// the routine of the real Rust crewmate_list reply, and the phone keeps it (to show it) without sending it back.
// Regenerate: UPDATE_FIXTURES=1 cargo test -p atlas-host automations_an_older_parser_accepted

void main() {
  late CrewmateEntry entry;

  setUpAll(() {
    final raw = File('test/fixtures/crewmate_list_invalid_cron.json')
        .readAsStringSync();
    final fixture = jsonDecode(raw) as Map<String, dynamic>;
    entry = CrewmateEntry.fromJson(
      (fixture['crewmates'] as List).single as Map<String, dynamic>,
    );
  });

  test('only the refused trigger and routine carry the parser message', () {
    expect(entry.triggers.map((t) => t.invalid != null), [false, false, true]);
    expect(entry.routines.map((r) => r.invalid != null), [false, false, true]);
    expect(
      entry.triggers.last.invalid,
      startsWith("invalid cron '+5 * * * *': "),
    );
    expect(
      entry.routines.last.invalid,
      startsWith("invalid cron '+5 * * * *': "),
    );
  });

  test('a refused schedule has no next fire, a `?` one keeps its own', () {
    expect(entry.triggers[1].nextFire, isNotNull);
    expect(entry.triggers.last.nextFire, isNull);
  });

  test('the message is read-only: saving the list sends the cron as saved', () {
    expect(entry.triggers.last.toJson().containsKey('invalid'), isFalse);
    expect(entry.routines.last.toJson().containsKey('invalid'), isFalse);
    expect(entry.triggers.last.toJson()['kind'], {
      'type': 'schedule',
      'cron': '+5 * * * *',
    });
  });
}
