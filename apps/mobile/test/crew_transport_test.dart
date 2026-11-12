// A bot computer reached through the paired Mac (2026-11-12: the Crew tab was empty because
// the phone only talks to its Mac). The Mac's side is crates/atlas-host/src/crew.rs.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/transport/crew_transport.dart';

import 'fake_transport.dart';

void main() {
  late FakeTransport wire;
  late Multiplexer mac;
  late Multiplexer bot;

  setUp(() {
    wire = FakeTransport();
    mac = Multiplexer(wire);
    bot = Multiplexer(CrewTransport(mac, 'atlas-bot@ship'));
  });

  test(
    'a call goes to the Mac as crew_call and its answer comes back',
    () async {
      final reply = bot.call('bot_list', {'x': 1});
      await pumpEventQueue();
      final sent = jsonDecode(wire.sentLines.single) as Map<String, dynamic>;
      expect(sent['cmd'], 'crew_call');
      expect(sent['args'], {
        'server': 'atlas-bot@ship',
        'cmd': 'bot_list',
        'args': {'x': 1},
      });
      wire.injectLine(
        jsonEncode({
          'id': sent['id'],
          'ok': true,
          'data': {
            'bots': [
              {'name': 'Jorge'},
            ],
          },
        }),
      );
      final r = await reply;
      expect(r['ok'], true);
      expect((r['data'] as Map)['bots'], [
        {'name': 'Jorge'},
      ]);
    },
  );

  test('the Mac refusing the relay is the call failing', () async {
    final reply = bot.call('bot_list');
    await pumpEventQueue();
    final sent = jsonDecode(wire.sentLines.single) as Map<String, dynamic>;
    wire.injectLine(
      jsonEncode({'id': sent['id'], 'ok': false, 'error': 'forbidden: act'}),
    );
    expect((await reply)['error'], 'forbidden: act');
  });

  test("only this server's relayed pushes arrive, unwrapped", () async {
    final got = <Map<String, dynamic>>[];
    bot.pushEvents.listen(got.add);
    for (final (server, ev) in [
      ('atlas-bot@ship', 'bot_task'),
      ('atlas-bot@other', 'bot_task'),
      ('atlas-bot@ship', 'agent_approval'),
    ]) {
      wire.injectLine(
        jsonEncode({
          'ev': 'crew_event',
          'server': server,
          'event': {'ev': ev, 'bot_id': 'b'},
        }),
      );
    }
    // The Mac's own pushes are not the bot computer's.
    wire.injectLine(jsonEncode({'ev': 'agent_status', 'pane': 'p'}));
    await pumpEventQueue();
    expect(got.map((e) => e['ev']), ['bot_task', 'agent_approval']);
    expect(got.first['bot_id'], 'b');
  });

  test('the bot computer is gone when the Mac connection drops', () async {
    final pending = bot.call('bot_list');
    await pumpEventQueue();
    await wire.simulateRemoteClose();
    expect((await pending)['ok'], false);
    await expectLater(bot.done, completes);
  });
}
