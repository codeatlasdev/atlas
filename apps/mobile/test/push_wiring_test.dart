// Issue #205: the phone must (re-)register its FCM token, with the relay's grant, with every
// connected computer that can notify it: on first connect, when FCM rotates the token, and after
// an upgrade (a token a pre-grant app registered has no grant and is never notified until this runs).
import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_provider.dart';
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
class _Computer implements Transport {
  final registered = <Map<String, dynamic>>[];
  var caps = ['mobile', 'crew'];
  final _lines = StreamController<String>.broadcast();

  @override
  Stream<String> get incomingLines => _lines.stream;

  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    final cmd = req['cmd'];
    if (cmd == 'push_register') registered.add((req['args'] as Map).cast());
    _lines.add(
      jsonEncode({
        'id': req['id'],
        'ok': true,
        'data': cmd == 'info' ? {'caps': caps} : {},
      }),
    );
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

  ProviderContainer container(List<MacDevice> devices) {
    final c = ProviderContainer(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage(devices)),
        transportFactoryProvider.overrideWithValue((_, _) async => computer),
        pushGrantFetcherProvider.overrideWithValue((token) async {
          asked.add(token);
          return 'grant:$token';
        }),
      ],
    );
    addTearDown(c.dispose);
    return c;
  }

  setUp(() {
    computer = _Computer();
    asked = [];
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
}
