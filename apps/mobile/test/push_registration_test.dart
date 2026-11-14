import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/push/push_registration.dart';

import 'fake_transport.dart';

void main() {
  test('hostHasCrewCap returns false for null data', () {
    expect(hostHasCrewCap(null), isFalse);
  });

  test('hostHasCrewCap returns false when caps absent', () {
    expect(hostHasCrewCap({'version': '1.0'}), isFalse);
  });

  test('hostHasCrewCap returns false when caps is empty list', () {
    expect(hostHasCrewCap({'caps': <String>[]}), isFalse);
  });

  test('hostHasCrewCap returns false when caps lacks crew', () {
    expect(
      hostHasCrewCap({
        'caps': ['mobile', 'agents'],
      }),
      isFalse,
    );
  });

  test('hostHasCrewCap returns true when caps includes crew', () {
    expect(
      hostHasCrewCap({
        'caps': ['mobile', 'crew', 'agents'],
      }),
      isTrue,
    );
  });

  test('hostHasCrewCap returns true when crew is the only cap', () {
    expect(
      hostHasCrewCap({
        'caps': ['crew'],
      }),
      isTrue,
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
          return 'grant-for-fcm-token-1';
        });
        final req = await answerNext();
        expect(await done, isTrue);
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
    // better to register nothing and try again on the next connect.
    test('registers nothing when the relay gives no grant', () async {
      expect(await registerPush(mux, 't', (_) async => null), isFalse);
      expect(transport.sentLines, isEmpty);
    });

    test('a relay that throws is not a crash, and registers nothing', () async {
      expect(
        await registerPush(
          mux,
          't',
          (_) async => throw const SocketException('offline'),
        ),
        isFalse,
      );
      expect(transport.sentLines, isEmpty);
    });

    test('reports a refused push_register as not registered', () async {
      final done = registerPush(mux, 't', (_) async => 'g');
      while (transport.sentLines.isEmpty) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }
      final req = jsonDecode(transport.sentLines.last) as Map<String, dynamic>;
      transport.injectLine(
        jsonEncode({'id': req['id'], 'ok': false, 'error': 'forbidden'}),
      );
      expect(await done, isFalse);
    });

    // Contract: hostd's `HostCmd::PushRegister` parses exactly these args
    // (crates/atlas-host/src/crew/tests/push.rs reads this file). Written from the args the code
    // really builds, never by hand: UPDATE_FIXTURES=1 flutter test test/push_registration_test.dart
    test('the args on the wire match the contract fixture', () async {
      final done = registerPush(mux, 'fcm-token-1', (_) async => 'grant-1');
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
}
