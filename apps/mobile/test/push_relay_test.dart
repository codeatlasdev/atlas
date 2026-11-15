// Issue #205: the phone's side of POST /v1/register against a real local HTTP server.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/push/push_result.dart';
import 'package:atlas_mobile/core/push/push_relay.dart';

Future<(HttpServer, List<String>)> relay(int status, String body) async {
  final s = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
  final seen = <String>[];
  s.listen((req) async {
    seen.add('${req.method} ${req.uri.path} ${await utf8.decodeStream(req)}');
    req.response
      ..statusCode = status
      ..write(body);
    await req.response.close();
  });
  addTearDown(() => s.close(force: true));
  return (s, seen);
}

void main() {
  test('posts the token to /v1/register and returns the grant', () async {
    final (s, seen) = await relay(200, '{"grant":"abc"}');
    // A trailing slash on the configured relay must not produce //v1/register.
    final r = await fetchPushGrant(
      'fcm-1',
      relay: 'http://127.0.0.1:${s.port}/',
    );
    expect(r, isA<PushOk>());
    expect((r as PushOk).value, 'abc');
    expect(seen, ['POST /v1/register {"token":"fcm-1"}']);
  });

  // A failing relay is a typed failure with the reason the user can act on, never a bare null.
  test('each failing relay status says why', () async {
    final reasons = {
      429: 'too many',
      403: 'refused',
      400: 'refused',
      500: 'relay is down',
      503: 'relay is down',
    };
    for (final e in reasons.entries) {
      final (s, _) = await relay(e.key, 'nope');
      final r = await fetchPushGrant('t', relay: 'http://127.0.0.1:${s.port}');
      expect(r, isA<PushFail>(), reason: '${e.key}');
      final why = (r as PushFail).reason;
      expect(why.toLowerCase(), contains(e.value), reason: '${e.key}: $why');
      expect(why, contains('${e.key}'), reason: 'the status is shown');
    }
  });

  test(
    'a 200 without a usable grant says the relay answer was unusable',
    () async {
      for (final body in ['{}', '{"grant":""}', '{"grant":3}', 'not json']) {
        final (s, _) = await relay(200, body);
        final r = await fetchPushGrant(
          't',
          relay: 'http://127.0.0.1:${s.port}',
        );
        expect(r, isA<PushFail>(), reason: body);
        expect((r as PushFail).reason.toLowerCase(), contains('unexpected'));
      }
    },
  );

  test('an unreachable relay says it could not be reached', () async {
    final r = await fetchPushGrant('t', relay: 'http://127.0.0.1:1');
    expect(r, isA<PushFail>());
    expect((r as PushFail).reason.toLowerCase(), contains('reach'));
  });

  test('a relay that never answers says it timed out', () async {
    final s = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    s.listen((_) {}); // accepts, never answers
    addTearDown(() => s.close(force: true));
    final r = await fetchPushGrant(
      't',
      relay: 'http://127.0.0.1:${s.port}',
      timeout: const Duration(milliseconds: 150),
    );
    expect(r, isA<PushFail>());
    expect((r as PushFail).reason.toLowerCase(), contains('did not answer'));
  });
}
