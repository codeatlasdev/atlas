import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:atlas_mobile/core/push/push_result.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/push/push_registration.dart';

import 'fake_transport.dart';

void main() {
  test('hostCanPush returns false for null data', () {
    expect(hostCanPush(null), isFalse);
  });

  test('hostCanPush returns false when caps absent', () {
    expect(hostCanPush({'version': '1.0'}), isFalse);
  });

  test('hostCanPush returns false when caps is empty list', () {
    expect(hostCanPush({'caps': <String>[]}), isFalse);
  });

  test('hostCanPush returns false when caps lacks crew', () {
    expect(
      hostCanPush({
        'caps': ['mobile', 'agents'],
      }),
      isFalse,
    );
  });

  test('hostCanPush returns true when caps includes crew', () {
    expect(
      hostCanPush({
        'caps': ['mobile', 'crew', 'agents'],
      }),
      isTrue,
    );
  });

  test('hostCanPush returns true when crew is the only cap', () {
    expect(
      hostCanPush({
        'caps': ['crew'],
      }),
      isTrue,
    );
  });

  // A plain Mac pushes about its own agents and says so with `push`, crew server or not.
  test('hostCanPush returns true for a plain Mac that advertises push', () {
    expect(
      hostCanPush({
        'caps': ['pty_attach_raw', 'lsp', 'agents', 'push', 'upgrade'],
      }),
      isTrue,
    );
  });

  test('hostCanPush returns false for a hostd from before the push cap', () {
    expect(
      hostCanPush({
        'caps': ['pty_attach_raw', 'lsp', 'agents', 'upgrade'],
      }),
      isFalse,
    );
  });

  // Issue #205: the relay refuses a notify without the grant it issued for the token, so the phone
  // must fetch the grant and hand it to the crew server together with the token.
  group('registerPush', () {
    late FakeTransport transport;
    late Multiplexer mux;

    setUp(() {
      transport = FakeTransport();
      mux = Multiplexer(transport);
    });
    tearDown(() => mux.close());

    /// Answers the next request the phone sent with ok.
    Future<Map<String, dynamic>> answerNext() async {
      while (transport.sentLines.isEmpty) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }
      final req = jsonDecode(transport.sentLines.last) as Map<String, dynamic>;
      transport.injectLine(
        jsonEncode({'id': req['id'], 'ok': true, 'data': {}}),
      );
      return req;
    }

    test(
      'asks the relay for a grant and sends token, platform and grant',
      () async {
        final asked = <String>[];
        final done = registerPush(mux, 'fcm-token-1', (token) async {
          asked.add(token);
          return const PushOk('grant-for-fcm-token-1');
        });
        final req = await answerNext();
        expect(await done, isA<PushOk>());
        expect(asked, ['fcm-token-1']);
        expect(req['cmd'], 'push_register');
        expect(req['args'], {
          'token': 'fcm-token-1',
          'platform': 'fcm',
          'grant': 'grant-for-fcm-token-1',
        });
      },
    );

    // The server would store an entry the relay can never accept (and drop it on the first 403);
    // better to register nothing and try again on the next connect. The reason is the relay's.
    test(
      'registers nothing, and says why, when the relay gives no grant',
      () async {
        final r = await registerPush(
          mux,
          't',
          (_) async => const PushFail('The push relay is down (HTTP 503)'),
        );
        expect(r, isA<PushFail>());
        expect((r as PushFail).reason, contains('HTTP 503'));
        expect(transport.sentLines, isEmpty);
      },
    );

    test('a relay that throws is not a crash, and registers nothing', () async {
      final r = await registerPush(
        mux,
        't',
        (_) async => throw const SocketException('offline'),
      );
      expect(r, isA<PushFail>());
      expect((r as PushFail).reason, contains('offline'));
      expect(transport.sentLines, isEmpty);
    });

    test('a refused push_register carries the computer\'s own error', () async {
      final done = registerPush(mux, 't', (_) async => const PushOk('g'));
      while (transport.sentLines.isEmpty) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }
      final req = jsonDecode(transport.sentLines.last) as Map<String, dynamic>;
      transport.injectLine(
        jsonEncode({'id': req['id'], 'ok': false, 'error': 'forbidden: act'}),
      );
      final r = await done;
      expect(r, isA<PushFail>());
      expect((r as PushFail).reason, contains('forbidden: act'));
    });

    // Contract: hostd's `HostCmd::PushRegister` parses exactly these args
    // (crates/atlas-host/src/crew/tests/push.rs reads this file). Written from the args the code
    // really builds, never by hand: UPDATE_FIXTURES=1 flutter test test/push_registration_test.dart
    test('the args on the wire match the contract fixture', () async {
      final done = registerPush(
        mux,
        'fcm-token-1',
        (_) async => const PushOk('grant-1'),
      );
      final req = await answerNext();
      await done;
      final file = File('test/fixtures/push_register.json');
      final live = const JsonEncoder.withIndent('  ').convert(req['args']);
      if (Platform.environment.containsKey('UPDATE_FIXTURES')) {
        file.writeAsStringSync('$live\n');
      }
      expect(jsonDecode(file.readAsStringSync()), req['args']);
    });
  });

  group('checkCanPush', () {
    late FakeTransport transport;
    late Multiplexer mux;

    setUp(() {
      transport = FakeTransport();
      mux = Multiplexer(transport, timeout: const Duration(milliseconds: 200));
    });
    tearDown(() => mux.close());

    Future<PushResult<void>> askWith(Map<String, dynamic> reply) async {
      final done = checkCanPush(mux);
      while (transport.sentLines.isEmpty) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }
      final req = jsonDecode(transport.sentLines.last) as Map<String, dynamic>;
      transport.injectLine(jsonEncode({'id': req['id'], ...reply}));
      return done;
    }

    test('a host with the push cap can push', () async {
      final r = await askWith({
        'ok': true,
        'data': {
          'caps': ['push'],
        },
      });
      expect(r, isA<PushOk>());
    });

    test('a host from before the push cap says it needs an update', () async {
      final r = await askWith({
        'ok': true,
        'data': {
          'caps': ['mobile', 'agents'],
        },
      });
      expect(r, isA<PushFail>());
      expect((r as PushFail).reason.toLowerCase(), contains('update atlas'));
    });

    test('a host that refuses info carries its error', () async {
      final r = await askWith({'ok': false, 'error': 'forbidden: read'});
      expect(r, isA<PushFail>());
      expect((r as PushFail).reason, contains('forbidden: read'));
    });

    test('a host that does not answer says so', () async {
      final r = await checkCanPush(mux);
      expect(r, isA<PushFail>());
      expect((r as PushFail).reason.toLowerCase(), contains('did not answer'));
    });
  });
}
