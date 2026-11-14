// Issue #205: the phone's side of POST /v1/register against a real local HTTP server.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

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
    final grant = await fetchPushGrant(
      'fcm-1',
      relay: 'http://127.0.0.1:${s.port}/',
    );
    expect(grant, 'abc');
    expect(seen, ['POST /v1/register {"token":"fcm-1"}']);
  });

  test(
    'a rate-limited or failing relay is no grant, not an exception',
    () async {
      for (final status in [429, 400, 500]) {
        final (s, _) = await relay(status, 'nope');
        expect(
          await fetchPushGrant('t', relay: 'http://127.0.0.1:${s.port}'),
          isNull,
        );
      }
    },
  );

  test('a 200 without a usable grant is no grant', () async {
    for (final body in ['{}', '{"grant":""}', '{"grant":3}', 'not json']) {
      final (s, _) = await relay(200, body);
      expect(
        await fetchPushGrant('t', relay: 'http://127.0.0.1:${s.port}'),
        isNull,
        reason: body,
      );
    }
  });

  test('an unreachable relay is no grant', () async {
    expect(await fetchPushGrant('t', relay: 'http://127.0.0.1:1'), isNull);
  });
}
