// A crew server reached through the paired Mac (2026-11-12: the Crew tab was empty because
// the phone only talks to its Mac). The Mac's side is crates/atlas-host/src/crew/relay.rs.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/transport/crew_transport.dart';

import 'fake_transport.dart';

void main() {
  late FakeTransport wire;
  late Multiplexer mac;
  late Multiplexer crewmate;

  setUp(() {
    wire = FakeTransport();
    mac = Multiplexer(wire);
    crewmate = Multiplexer(CrewTransport(mac, 'atlas-crew@server'));
  });

  test(
    'a call goes to the Mac as crew_call and its answer comes back',
    () async {
      final reply = crewmate.call('crewmate_list', {'x': 1});
      await pumpEventQueue();
      final sent = jsonDecode(wire.sentLines.single) as Map<String, dynamic>;
      expect(sent['cmd'], 'crew_call');
      expect(sent['args'], {
        'server': 'atlas-crew@server',
        'cmd': 'crewmate_list',
        'args': {'x': 1},
      });
      wire.injectLine(
        jsonEncode({
          'id': sent['id'],
          'ok': true,
          'data': {
            'crewmates': [
              {'name': 'Jorge'},
            ],
          },
        }),
      );
      final r = await reply;
      expect(r['ok'], true);
      expect((r['data'] as Map)['crewmates'], [
        {'name': 'Jorge'},
      ]);
    },
  );

  test('the Mac refusing the relay is the call failing', () async {
    final reply = crewmate.call('crewmate_list');
    await pumpEventQueue();
    final sent = jsonDecode(wire.sentLines.single) as Map<String, dynamic>;
    wire.injectLine(
      jsonEncode({'id': sent['id'], 'ok': false, 'error': 'forbidden: act'}),
    );
    expect((await reply)['error'], 'forbidden: act');
  });

  test("only this server's relayed pushes arrive, unwrapped", () async {
    final got = <Map<String, dynamic>>[];
    crewmate.pushEvents.listen(got.add);
    for (final (server, ev) in [
      ('atlas-crew@server', 'crewmate_task'),
      ('atlas-crew@other', 'crewmate_task'),
      ('atlas-crew@server', 'agent_approval'),
    ]) {
      wire.injectLine(
        jsonEncode({
          'ev': 'crew_event',
          'server': server,
          'event': {'ev': ev, 'crewmate_id': 'b'},
        }),
      );
    }
    // The Mac's own pushes are not the crew server's.
    wire.injectLine(jsonEncode({'ev': 'agent_status', 'pane': 'p'}));
    await pumpEventQueue();
    expect(got.map((e) => e['ev']), ['crewmate_task', 'agent_approval']);
    expect(got.first['crewmate_id'], 'b');
  });

  test('the crew server is gone when the Mac connection drops', () async {
    final pending = crewmate.call('crewmate_list');
    await pumpEventQueue();
    await wire.simulateRemoteClose();
    expect((await pending)['ok'], false);
    await expectLater(crewmate.done, completes);
  });
}
