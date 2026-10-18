import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/transport/fake_transport.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';

void main() {
  group('Multiplexer', () {
    late FakeTransport ft;
    late Multiplexer mux;

    setUp(() {
      ft = FakeTransport();
      mux = Multiplexer(ft);
    });

    tearDown(() async {
      await mux.close();
    });

    // ── Reply routing ─────────────────────────────────────────────────────────

    test('routes reply to the matching pending call by id', () async {
      final future = mux.call('ping');
      // The call allocates id=1; inject the matching response.
      ft.injectLine('{"id":1,"ok":true,"data":{}}');
      final resp = await future;
      expect(resp['ok'], isTrue);
    });

    test('routes two concurrent calls to their respective ids', () async {
      final f1 = mux.call('ping');
      final f2 = mux.call('pong');
      // id=2 resolves first; id=1 resolves after.
      ft.injectLine('{"id":2,"ok":true,"data":{"seq":2}}');
      ft.injectLine('{"id":1,"ok":true,"data":{"seq":1}}');
      final r1 = await f1;
      final r2 = await f2;
      expect(r1['data']['seq'], 1);
      expect(r2['data']['seq'], 2);
    });

    // ── Push events ───────────────────────────────────────────────────────────

    test('push events reach pushEvents stream', () async {
      final eventFuture = mux.pushEvents.first;
      ft.injectLine(
        '{"ev":"agent_status","pane_key":"p1","status":{"state":"idle"}}',
      );
      final event = await eventFuture;
      expect(event['ev'], 'agent_status');
      expect(event['pane_key'], 'p1');
    });

    test('push events and replies are dispatched independently', () async {
      final pushes = <Map<String, dynamic>>[];
      final sub = mux.pushEvents.listen(pushes.add);

      final callFuture = mux.call('ping'); // id=1

      ft.injectLine('{"ev":"agent_status","pane_key":"p","status":{}}');
      ft.injectLine('{"id":1,"ok":true,"data":{}}');

      await callFuture;
      await Future.delayed(Duration.zero);

      expect(pushes, hasLength(1));
      expect(pushes.first['ev'], 'agent_status');
      await sub.cancel();
    });

    // ── Line size limit (>1 MiB rejected) ────────────────────────────────────

    test('drops lines longer than 1 MiB and does not route them', () async {
      final pushes = <Map<String, dynamic>>[];
      final sub = mux.pushEvents.listen(pushes.add);

      // Inject an oversized "push" line — it must be silently dropped.
      final giant = '{"ev":"too_big","data":"${'x' * (1024 * 1024)}"}';
      expect(giant.length, greaterThan(Multiplexer.maxLineBytes));
      ft.injectLine(giant);

      // Inject a normal push event; it must arrive.
      ft.injectLine('{"ev":"ok_event"}');
      await Future.delayed(Duration.zero);

      expect(pushes, hasLength(1));
      expect(pushes.first['ev'], 'ok_event');

      await sub.cancel();
    });

    test('oversized line does not resolve a pending call; subsequent valid '
        'response does', () async {
      final callFuture = mux.call('ping'); // id=1
      final giant = 'x' * (Multiplexer.maxLineBytes + 1);
      ft.injectLine(giant); // dropped
      ft.injectLine('{"id":1,"ok":true,"data":{}}');
      final resp = await callFuture;
      expect(resp['ok'], isTrue);
    });
  });
}
