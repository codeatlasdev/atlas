import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';

import 'support/fake_permissions.dart';

// iOS shows its Local Network prompt on the first packet to a local address. Until the person was told why,
// the phone must not hand the connection the computer's LAN addresses.

const _mac = MacDevice(
  nodeId: 'n1',
  name: 'Mac',
  addrs: ['192.168.1.20:4000'],
  relay: 'https://relay.example',
);

Future<List<String>> _dialled(PermissionState? state) async {
  late MacDevice seen;
  final dial = withLocalNetworkGate(
    FakePermissions(states: {AtlasPermission.localNetwork: ?state}),
    (device, _) async {
      seen = device;
      return _NoTransport();
    },
  );
  await dial(_mac, 'key');
  expect(
    seen.relay,
    'https://relay.example',
    reason: 'the relay is always kept',
  );
  return seen.addrs;
}

class _NoTransport implements Transport {
  @override
  Stream<String> get incomingLines => const Stream.empty();
  @override
  Future<void> sendLine(String line) async {}
  @override
  Future<void> close() async {}
}

void main() {
  test('not explained yet: no LAN address goes to the dial', () async {
    expect(await _dialled(PermissionState.notAsked), isEmpty);
  });

  test('explained: the LAN addresses are used', () async {
    expect(await _dialled(PermissionState.granted), ['192.168.1.20:4000']);
  });

  test(
    'where there is no such prompt (Android): the addresses are used',
    () async {
      expect(await _dialled(PermissionState.unavailable), [
        '192.168.1.20:4000',
      ]);
    },
  );
}
