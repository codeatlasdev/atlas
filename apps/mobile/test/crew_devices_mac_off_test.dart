import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';

import 'support/demo_harness.dart';

// The phone is PC-free only if a crew server paired directly stays listed when the Mac is off or asleep
// (PhoneCrewAudit gap 4). `crewDevicesProvider` awaited each Mac's `crew_servers` before returning anything,
// so a Mac whose link was still "up" but never answered (asleep) hid every directly paired crew server for
// as long as the call took to fail.

const _mac = MacDevice(nodeId: 'mac-1', name: 'Mac');
const _server = MacDevice(nodeId: 'srv-1', name: 'Ship');

/// A computer whose link is open but that answers `crew_servers` only when [answer] says so (`null`: never).
class _Computer implements Transport {
  _Computer({this.relayed});
  final List<Map<String, String>>? relayed;
  final _in = StreamController<String>.broadcast();
  @override
  Stream<String> get incomingLines => _in.stream;
  @override
  Future<void> close() => _in.close();
  @override
  Future<void> sendLine(String line) async {
    final m = jsonDecode(line) as Map<String, dynamic>;
    final asleep = m['cmd'] == 'crew_servers' && relayed == null;
    // Asleep: no answer, ever.
    if (asleep) {
      return;
    }
    final data = switch (m['cmd']) {
      'crew_servers' => {'servers': relayed},
      // The connection check on connect: a current hostd names its protocol.
      'info' => {'proto': kAppProto},
      _ => <String, Object?>{},
    };
    scheduleMicrotask(
      () => _in.add(jsonEncode({'id': m['id'], 'ok': true, 'data': data})),
    );
  }
}

ProviderContainer _container(Map<String, Transport> by) {
  final c = ProviderContainer(
    overrides: [
      macStorageProvider.overrideWithValue(DemoStorage([_mac, _server])),
      transportFactoryProvider.overrideWithValue((d, _) async => by[d.nodeId]!),
    ],
  );
  addTearDown(c.dispose);
  return c;
}

void main() {
  test(
    'a directly paired crew server is listed while its Mac never answers',
    () async {
      final c = _container({
        'mac-1': _Computer(),
        'srv-1': _Computer(relayed: []),
      });
      final sub = c.listen(crewDevicesProvider, (_, _) {});
      addTearDown(sub.close);
      final t0 = DateTime.now();
      final devices = await c
          .read(crewDevicesProvider.future)
          .timeout(const Duration(seconds: 8));
      // Listed at once, not after the Mac's 20 s request deadline.
      expect(
        DateTime.now().difference(t0),
        lessThan(const Duration(seconds: 3)),
      );
      expect(devices.map((d) => d.nodeId), containsAll(['mac-1', 'srv-1']));
    },
  );

  test('crew servers a Mac relays are still added once it answers', () async {
    final c = _container({
      'mac-1': _Computer(
        relayed: [
          {'id': 'ubuntu@box', 'name': 'Box'},
        ],
      ),
      'srv-1': _Computer(relayed: []),
    });
    final sub = c.listen(crewDevicesProvider, (_, _) {});
    addTearDown(sub.close);
    // The list is rebuilt when the Mac's connection comes up: wait for the relayed server to appear.
    var devices = await c.read(crewDevicesProvider.future);
    for (var i = 0; i < 50 && !devices.any((d) => d.name == 'Box'); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 100));
      devices = await c.read(crewDevicesProvider.future);
    }
    expect(devices.map((d) => d.name), containsAll(['Mac', 'Ship', 'Box']));
    expect(devices.firstWhere((d) => d.name == 'Box').via?.nodeId, 'mac-1');
  });
}
