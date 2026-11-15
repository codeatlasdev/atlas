import 'package:flutter_test/flutter_test.dart';

import 'fake_transport.dart';

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

    // ── A dropped reply fails ITS call at once, with the real cause ──────────
    // hostd builds a reply with serde_json `json!`: `{"id":..` first, or (without
    // `preserve_order`) keys sorted, `data`/`error` first and `id`,`ok` last. A reply we cannot
    // read used to leave the call waiting 20 s for "Your Mac did not answer".

    Future<Map<String, dynamic>> answeredWithin(
      Future<Map<String, dynamic>> f,
    ) => f.timeout(const Duration(seconds: 2), onTimeout: () => {'hung': true});

    test('an oversized reply (id first) fails its call at once, saying it was '
        'too large', () async {
      final f = mux.call('project_detail'); // id=1
      ft.injectLine('{"id":1,"ok":true,"data":"${'x' * (1024 * 1024)}"}');
      final resp = await answeredWithin(f);
      expect(resp['hung'], isNull, reason: 'call waited for the 20 s deadline');
      expect(resp['ok'], isFalse);
      expect(resp['error'], contains('too large'));
      expect(resp['error'], contains('MB'));
      expect(mux.droppedLines, 0, reason: 'attributed to a call, not lost');
    });

    test('an oversized reply with sorted keys (id near the end) fails its '
        'call too', () async {
      final f = mux.call('project_detail'); // id=1
      ft.injectLine('{"data":"${'x' * (1024 * 1024)}","id":1,"ok":true}');
      final resp = await answeredWithin(f);
      expect(resp['hung'], isNull);
      expect(resp['ok'], isFalse);
      expect(resp['error'], contains('too large'));
    });

    test(
      'an oversized error reply with sorted keys fails its call too',
      () async {
        final f = mux.call('x'); // id=1
        ft.injectLine('{"error":"${'e' * (1024 * 1024)}","id":1,"ok":false}');
        final resp = await answeredWithin(f);
        expect(resp['hung'], isNull);
        expect(resp['error'], contains('too large'));
      },
    );

    test('only the call whose id is on the line fails; the other keeps '
        'waiting for its reply', () async {
      final f1 = mux.call('a'); // id=1
      final f2 = mux.call('b'); // id=2
      ft.injectLine('{"id":2,"ok":true,"data":"${'x' * (1024 * 1024)}"}');
      final r2 = await answeredWithin(f2);
      expect(r2['error'], contains('too large'));
      ft.injectLine('{"id":1,"ok":true,"data":{"fine":1}}');
      final r1 = await answeredWithin(f1);
      expect(r1['ok'], isTrue);
      expect(r1['data']['fine'], 1);
    });

    // A nested object carries "id" too: only the line's own envelope counts, so a reply is never
    // attributed to a nested id (that would fail someone else's call).
    test('an id inside the payload is never taken for the reply id', () async {
      final f1 = mux.call('a'); // id=1
      final f7 = mux.call(
        'b',
      ); // id=2 (the nested "id":7 must not match anything real)
      final big = 'x' * (1024 * 1024);
      // Envelope id is 1 (sorted shape); a nested {"id":2,"ok":true} sits in the middle.
      ft.injectLine(
        '{"data":{"rows":[{"id":2,"ok":true}],"p":"$big"},"id":1,"ok":true}',
      );
      final r1 = await answeredWithin(f1);
      expect(r1['error'], contains('too large'));
      ft.injectLine('{"id":2,"ok":true,"data":{}}');
      expect((await answeredWithin(f7))['ok'], isTrue);
    });

    test('a malformed reply (id first) fails its call, saying the reply was '
        'unreadable', () async {
      final f = mux.call('ping'); // id=1
      ft.injectLine('{"id":1,"ok":true,"data":{oops');
      final resp = await answeredWithin(f);
      expect(resp['hung'], isNull);
      expect(resp['ok'], isFalse);
      expect(resp['error'], contains('malformed reply'));
    });

    test('a malformed reply (sorted keys) fails its call', () async {
      final f = mux.call('ping'); // id=1
      ft.injectLine('{"data":[1,2,,"id":1,"ok":true}');
      final resp = await answeredWithin(f);
      expect(resp['hung'], isNull);
      expect(resp['error'], contains('malformed reply'));
    });

    test('a dropped line with no recoverable id is counted, not lost silently, '
        'and the push stream still works', () async {
      expect(mux.droppedLines, 0);
      ft.injectLine('not json at all');
      ft.injectLine('x' * (Multiplexer.maxLineBytes + 1));
      await Future<void>.delayed(Duration.zero);
      expect(mux.droppedLines, 2);
      final pushes = <Map<String, dynamic>>[];
      final sub = mux.pushEvents.listen(pushes.add);
      ft.injectLine('{"ev":"ok_event"}');
      await Future<void>.delayed(Duration.zero);
      expect(pushes, hasLength(1));
      await sub.cancel();
    });

    test('a reply for an id nobody waits for (already timed out) is ignored, '
        'not counted as dropped', () async {
      ft.injectLine('{"id":99,"ok":true,"data":{}}');
      ft.injectLine('{"id":98,"ok":true,"data":"${'x' * (1024 * 1024)}"}');
      await Future<void>.delayed(Duration.zero);
      expect(mux.droppedLines, 0);
    });

    // Guards the endless pull-to-refresh spinner: a call whose reply never came
    // (connection dead without closing) waited forever.
    test(
      'a call with no reply fails with a timeout instead of hanging',
      () async {
        final m = Multiplexer(ft, timeout: const Duration(milliseconds: 50));
        final resp = await m
            .call('project_detail')
            .timeout(
              const Duration(seconds: 2),
              onTimeout: () => {'hung': true},
            );
        expect(resp['hung'], isNull, reason: 'call never completed');
        expect(resp['ok'], isFalse);
        expect(resp['error'], contains('did not answer'));
        // A reply that turns up late is dropped, not delivered to a finished call.
        ft.injectLine('{"id":1,"ok":true,"data":{}}');
        await Future<void>.delayed(Duration.zero);
      },
    );

    // Guards the disabled send button: when the transport was closed, call() was
    // throwing instead of returning an error map, so `_sending` was never reset.
    test(
      'a call on a closed transport returns an error map, not a thrown error',
      () async {
        await ft.simulateRemoteClose();
        // Attempting to call after the transport closes must never throw.
        Map<String, dynamic>? resp;
        Object? thrown;
        try {
          resp = await mux.call('ping');
        } catch (e) {
          thrown = e;
        }
        expect(thrown, isNull, reason: 'must not throw');
        expect(resp, isNotNull);
        expect(resp!['ok'], isFalse);
      },
    );

    // Guards the same spinner bug via the _onDone path: transport closes while a
    // call is in flight.  The completer must complete (not completeError).
    test(
      'a pending call whose transport closes mid-flight returns an error map',
      () async {
        final callFuture = mux.call('ping'); // id=1, now in-flight
        // Close the transport before the reply arrives.
        await ft.simulateRemoteClose();
        Map<String, dynamic>? resp;
        Object? thrown;
        try {
          resp = await callFuture;
        } catch (e) {
          thrown = e;
        }
        expect(thrown, isNull, reason: 'must not throw');
        expect(resp, isNotNull);
        expect(resp!['ok'], isFalse);
      },
    );
  });
}
