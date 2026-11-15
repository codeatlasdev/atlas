// Issue #205: the phone must (re-)register its FCM token, with the relay's grant, with every
// connected computer that can notify it: on first connect, when FCM rotates the token, and after
// an upgrade (a token a pre-grant app registered has no grant and is never notified until this runs).
import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_provider.dart';
import 'package:atlas_mobile/core/push/push_result.dart';
import 'package:atlas_mobile/core/push/push_service.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';

import 'package:atlas_mobile/core/transport/transport.dart';

class _Storage extends MacStorage {
  _Storage(this.devices) : super(storage: null);
  final List<MacDevice> devices;
  @override
  Future<List<MacDevice>> loadDevices() async => devices;
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// A computer that answers `info` as a crew host and ok to everything else; records push_register.
/// As a Mac it also lists one crew server (`crew_servers`) and relays `crew_call` to [server].
class _Computer implements Transport {
  final registered = <Map<String, dynamic>>[];
  var caps = ['mobile', 'crew'];
  _Computer? server;
  final _lines = StreamController<String>.broadcast();

  @override
  Stream<String> get incomingLines => _lines.stream;

  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    final cmd = req['cmd'];
    if (cmd == 'push_register') registered.add((req['args'] as Map).cast());
    Object data = cmd == 'info' ? {'proto': kAppProto, 'caps': caps} : {};
    if (cmd == 'crew_servers') {
      data = {
        'servers': [
          if (server != null) {'id': 'box', 'name': 'Box'},
        ],
      };
    }
    if (cmd == 'crew_call') {
      // Relay to the crew server behind this Mac, as hostd does.
      final inner = jsonEncode({'id': 1, ...(req['args'] as Map).cast()});
      final box = server!;
      final reply = await box.answer(inner);
      _lines.add(jsonEncode({...reply, 'id': req['id']}));
      return;
    }
    _lines.add(jsonEncode({'id': req['id'], 'ok': true, 'data': data}));
  }

  /// The reply of this computer to a request line, without going through the stream.
  Future<Map<String, dynamic>> answer(String line) async {
    final next = _lines.stream.first;
    await sendLine(line);
    return jsonDecode(await next) as Map<String, dynamic>;
  }

  @override
  Future<void> close() async {}
}

Future<void> _settle() async {
  for (var i = 0; i < 20; i++) {
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
}

void main() {
  const mac = MacDevice(nodeId: 'mac-1', name: 'Mac');
  late _Computer computer;
  late List<String> asked;
  PushResult<String> Function(String token)? grantFor;

  ProviderContainer container(List<MacDevice> devices) {
    final c = ProviderContainer(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage(devices)),
        transportFactoryProvider.overrideWithValue((_, _) async => computer),
        pushGrantFetcherProvider.overrideWithValue((token) async {
          asked.add(token);
          return grantFor?.call(token) ?? PushOk('grant:$token');
        }),
      ],
    );
    addTearDown(c.dispose);
    return c;
  }

  setUp(() {
    computer = _Computer();
    asked = [];
    grantFor = null;
  });

  test('registers the token and its grant with a connected computer', () async {
    final c = container([mac]);
    c.listen(pushRegistrationProvider, (_, _) {});
    c.read(fcmTokenProvider.notifier).state = 'fcm-1';
    await _settle();
    expect(asked, ['fcm-1']);
    expect(computer.registered, [
      {'token': 'fcm-1', 'platform': 'fcm', 'grant': 'grant:fcm-1'},
    ]);
  });

  test(
    're-registers, with a fresh grant, when FCM rotates the token',
    () async {
      final c = container([mac]);
      c.listen(pushRegistrationProvider, (_, _) {});
      c.read(fcmTokenProvider.notifier).state = 'fcm-1';
      await _settle();
      c.read(fcmTokenProvider.notifier).state = 'fcm-2';
      await _settle();
      expect(computer.registered.map((r) => r['token']), ['fcm-1', 'fcm-2']);
      expect(computer.registered.last['grant'], 'grant:fcm-2');
    },
  );

  // Crewmates, and so pushes, live on crew servers; the phone reaches them only through their Mac
  // (crew_call). The Mac itself is not a crew host here, the server behind it is.
  test('registers with a crew server behind a Mac, not with the Mac', () async {
    computer.caps = ['mobile'];
    computer.server = _Computer();
    final c = container([mac]);
    c.listen(pushRegistrationProvider, (_, _) {});
    c.read(fcmTokenProvider.notifier).state = 'fcm-1';
    await _settle();
    expect(computer.registered, isEmpty);
    expect(computer.server!.registered, [
      {'token': 'fcm-1', 'platform': 'fcm', 'grant': 'grant:fcm-1'},
    ]);
  });

  test(
    'does nothing while there is no FCM token (no Firebase config)',
    () async {
      final c = container([mac]);
      c.listen(pushRegistrationProvider, (_, _) {});
      await _settle();
      expect(asked, isEmpty);
      expect(computer.registered, isEmpty);
    },
  );

  // The demo computer is answered inside the app; there is nothing to notify and the relay must
  // not hear about a reviewer's phone.
  test('never registers with the demo computer', () async {
    final c = container([
      const MacDevice(nodeId: 'demo', name: 'Demo MacBook'),
    ]);
    c.listen(pushRegistrationProvider, (_, _) {});
    c.read(fcmTokenProvider.notifier).state = 'fcm-1';
    await _settle();
    expect(asked, isEmpty);
    expect(computer.registered, isEmpty);
  });

  // A host without the crew cap has no push sender: registering would be a wasted round trip
  // (and an error on an older hostd).
  test('skips a computer that is not a crew host', () async {
    computer.caps = ['mobile'];
    final c = container([mac]);
    c.listen(pushRegistrationProvider, (_, _) {});
    c.read(fcmTokenProvider.notifier).state = 'fcm-1';
    await _settle();
    expect(asked, isEmpty);
    expect(computer.registered, isEmpty);
  });

  // A phone paired only to a Mac gets pushes about that Mac's agents: a hostd that advertises `push`
  // takes the registration itself, whether or not it is a crew server.
  test(
    'registers with a Mac that advertises push, with no crew server',
    () async {
      computer.caps = ['mobile', 'agents', 'push'];
      final c = container([mac]);
      c.listen(pushRegistrationProvider, (_, _) {});
      c.read(fcmTokenProvider.notifier).state = 'fcm-1';
      await _settle();
      expect(computer.registered, [
        {'token': 'fcm-1', 'platform': 'fcm', 'grant': 'grant:fcm-1'},
      ]);
    },
  );

  // An older hostd (agents but no push cap) would answer an unknown command: do not ask it.
  test('skips a Mac from before the push cap', () async {
    computer.caps = ['mobile', 'agents', 'lsp'];
    final c = container([mac]);
    c.listen(pushRegistrationProvider, (_, _) {});
    c.read(fcmTokenProvider.notifier).state = 'fcm-1';
    await _settle();
    expect(asked, isEmpty);
    expect(computer.registered, isEmpty);
  });

  // The user could never tell why push did not work: every failure above was a silent null/false.
  // Now the app root's provider ends in one line that says the first reason.
  group('push status line', () {
    String? line(ProviderContainer c) => c.read(pushRegistrationProvider).value;

    test(
      'is null while there is no token (no Firebase config), nothing to say',
      () async {
        final c = container([mac]);
        c.listen(pushRegistrationProvider, (_, _) {});
        await _settle();
        expect(line(c), isNull);
      },
    );

    test(
      'says notifications are on once a computer took the registration',
      () async {
        final c = container([mac]);
        c.listen(pushRegistrationProvider, (_, _) {});
        c.read(fcmTokenProvider.notifier).state = 'fcm-1';
        await _settle();
        expect(line(c), contains('on'));
      },
    );

    test('says why when the relay refuses the grant', () async {
      grantFor = (_) => const PushFail('The push relay is down (HTTP 503)');
      final c = container([mac]);
      c.listen(pushRegistrationProvider, (_, _) {});
      c.read(fcmTokenProvider.notifier).state = 'fcm-1';
      await _settle();
      expect(line(c), contains('HTTP 503'));
      expect(computer.registered, isEmpty);
    });

    test('says a computer needs an update when it has no push cap', () async {
      computer.caps = ['mobile', 'agents'];
      final c = container([mac]);
      c.listen(pushRegistrationProvider, (_, _) {});
      c.read(fcmTokenProvider.notifier).state = 'fcm-1';
      await _settle();
      expect(line(c)!.toLowerCase(), contains('update atlas'));
    });

    // One working computer is enough for the phone to be notified; a failing one must not
    // hide that, but it is not "everything fine" either.
    test(
      'a failing computer does not stop the next one from registering',
      () async {
        computer.caps = ['mobile'];
        computer.server = _Computer();
        final c = container([mac]);
        c.listen(pushRegistrationProvider, (_, _) {});
        c.read(fcmTokenProvider.notifier).state = 'fcm-1';
        await _settle();
        expect(computer.server!.registered, hasLength(1));
        expect(line(c), contains('on'));
      },
    );

    test(
      'registers again, and the line follows, when the token rotates',
      () async {
        grantFor = (_) => const PushFail('relay unreachable');
        final c = container([mac]);
        c.listen(pushRegistrationProvider, (_, _) {});
        c.read(fcmTokenProvider.notifier).state = 'fcm-1';
        await _settle();
        expect(line(c), contains('relay unreachable'));
        grantFor = null;
        c.read(fcmTokenProvider.notifier).state = 'fcm-2';
        await _settle();
        expect(line(c), contains('on'));
      },
    );
  });
}
