// Contract: the Crew tab reads the paused state of a crew server from the real Rust crewmate_list reply.
// Incident: Crew review P0, PR #106 comment 6874910355 (no kill switch for a crew server).
// The fixture is written by the Rust serializer, never by hand:
// UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::pause
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/features/crew/crew_provider.dart';

void main() {
  late Map<String, dynamic> fixture;

  setUpAll(() {
    fixture = jsonDecode(
      File('test/fixtures/crewmate_list.json').readAsStringSync(),
    ) as Map<String, dynamic>;
  });

  test('the real crewmate_list carries a boolean paused next to crewmates', () {
    expect(fixture['paused'], isA<bool>());
    expect(fixture['crewmates'], isA<List<dynamic>>());
  });

  test('parseCrewList reads paused and the crewmates of the real reply', () {
    final r = parseCrewList(fixture, 'fallback');
    expect(r.isCrewServer, isTrue);
    expect(r.paused, fixture['paused']);
    expect(
      r.crewmates.map((c) => c.id),
      (fixture['crewmates'] as List).map((c) => (c as Map)['id']),
    );
  });

  test('a paused reply is paused, an unpaused one is not', () {
    expect(parseCrewList({...fixture, 'paused': true}, 'x').paused, isTrue);
    expect(parseCrewList({...fixture, 'paused': false}, 'x').paused, isFalse);
  });

  // A crew server older than the kill switch sends no field: it must not look paused, or the tab would
  // offer a Resume the server cannot honour.
  test('a reply without the field, or with a non-boolean, is not paused', () {
    final old = {...fixture}..remove('paused');
    expect(parseCrewList(old, 'x').paused, isFalse);
    expect(parseCrewList({...fixture, 'paused': 'true'}, 'x').paused, isFalse);
    expect(parseCrewList({...fixture, 'paused': 1}, 'x').paused, isFalse);
  });

  test('host_name falls back to the device name', () {
    expect(parseCrewList(fixture, 'My server').hostName, 'My server');
    expect(
      parseCrewList({...fixture, 'host_name': 'box'}, 'My server').hostName,
      'box',
    );
  });

  // The same event hostd pushes on every change (desktop fixture crew_paused_event.json).
  test('crew_paused refreshes the crew list like the task events do', () {
    expect(isCrewListPush({'ev': 'crew_paused', 'paused': true}), isTrue);
    expect(isCrewListPush({'ev': 'crewmate_task'}), isTrue);
    expect(isCrewListPush({'ev': 'crewmate_updated'}), isTrue);
    expect(isCrewListPush({'ev': 'agent_status:x'}), isTrue);
    expect(isCrewListPush({'ev': 'pty_output'}), isFalse);
  });
}
