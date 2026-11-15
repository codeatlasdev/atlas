// A phone that was offline when a crew server's invite was pushed used to miss it for good: the app only
// listened for the `computer_invite` push and never called `mobile_invites` (PhoneCrewAudit gaps 1 and 4).
// On connect it now asks every computer for its pending invites.
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

const _mac = MacDevice(nodeId: 'mac-1', name: 'Mac');
const _server = MacDevice(nodeId: 'srv-1', name: 'Ship');

/// A computer that answers `mobile_invites` with `invites` (hostd's real reply shape) and counts the asks.
class _Computer implements Transport {
  _Computer(this.invites);
  final List<Map<String, String>> invites;
  int asks = 0;
  final dismissed = <String>[];
  final _in = StreamController<String>.broadcast();

  @override
  Stream<String> get incomingLines => _in.stream;

  @override
  Future<void> close() => _in.close();

  void injectLine(String l) => _in.add(l);

  @override
  Future<void> sendLine(String line) async {
    final m = jsonDecode(line) as Map<String, dynamic>;
    final id = m['id'];
    Object? data = <String, Object?>{};
    switch (m['cmd']) {
      // The connection check on connect: a current hostd names its protocol.
      case 'info':
        data = {'proto': kAppProto};
      case 'mobile_invites':
        asks++;
        data = {'invites': invites};
      case 'mobile_invite_dismiss':
        dismissed.add((m['args'] as Map)['url'] as String);
        invites.removeWhere((i) => i['url'] == (m['args'] as Map)['url']);
    }
    scheduleMicrotask(
      () => injectLine(jsonEncode({'id': id, 'ok': true, 'data': data})),
    );
  }
}

Future<ProviderContainer> _container(Map<String, _Computer> computers) async {
  final c = ProviderContainer(
    overrides: [
      macStorageProvider.overrideWithValue(DemoStorage([_mac, _server])),
      transportFactoryProvider.overrideWithValue(
        (device, _) async => computers[device.nodeId]!,
      ),
    ],
  );
  addTearDown(c.dispose);
  return c;
}

void main() {
  test('every connected computer is asked, and each invite keeps the computer it came from', () async {
    final mac = _Computer([]);
    final server = _Computer([
      {
        'url': 'atlas-mobile://pair?node=N&code=111111&name=Ship',
        'name': 'Ship',
      },
    ]);
    final c = await _container({'mac-1': mac, 'srv-1': server});
    final sub = c.listen(pendingInvitesProvider, (_, _) {});
    addTearDown(sub.close);
    final found = await c.read(pendingInvitesProvider.future);
    expect(found, hasLength(1));
    expect(found.single.device.nodeId, 'srv-1');
    expect(found.single.invite.name, 'Ship');
    expect(found.single.invite.url, contains('code=111111'));
    expect(mac.asks, 1);
    expect(server.asks, 1);
  });

  test(
    'a computer that is offline or errors does not hide the others',
    () async {
      final server = _Computer([
        {
          'url': 'atlas-mobile://pair?node=N&code=222222&name=Ship',
          'name': 'Ship',
        },
      ]);
      final c = ProviderContainer(
        overrides: [
          macStorageProvider.overrideWithValue(DemoStorage([_mac, _server])),
          transportFactoryProvider.overrideWithValue((device, _) async {
            if (device.nodeId == 'mac-1') throw StateError('unreachable');
            return server;
          }),
        ],
      );
      addTearDown(c.dispose);
      final sub = c.listen(pendingInvitesProvider, (_, _) {});
      addTearDown(sub.close);
      final found = await c.read(pendingInvitesProvider.future);
      expect(found.map((f) => f.device.nodeId), ['srv-1']);
    },
  );

  test('no invites anywhere is an empty list, not an error', () async {
    final c = await _container({
      'mac-1': _Computer([]),
      'srv-1': _Computer([]),
    });
    final sub = c.listen(pendingInvitesProvider, (_, _) {});
    addTearDown(sub.close);
    expect(await c.read(pendingInvitesProvider.future), isEmpty);
  });
}
