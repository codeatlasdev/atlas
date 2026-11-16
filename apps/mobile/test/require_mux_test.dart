import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/transport/transport.dart';

import 'support/demo_harness.dart';

// `requireMux` is the one way a command gets its connection: the live Multiplexer, or ONE typed error
// that says why there is none (offline, or a protocol the app cannot speak). The sentence is defined once.

const _device = MacDevice(nodeId: 'mac-1', name: 'Studio');

/// A hostd that answers every request `ok`; `info` carries [proto].
class _Host implements Transport {
  _Host(this.proto);
  final int proto;
  final _in = StreamController<String>.broadcast();
  @override
  Stream<String> get incomingLines => _in.stream;
  @override
  Future<void> close() => _in.close();
  @override
  Future<void> sendLine(String line) async {
    final m = jsonDecode(line) as Map<String, dynamic>;
    final data = m['cmd'] == 'info' ? {'proto': proto} : <String, Object?>{};
    scheduleMicrotask(() {
      if (!_in.isClosed) {
        _in.add(jsonEncode({'id': m['id'], 'ok': true, 'data': data}));
      }
    });
  }
}

/// `requireMux` called once from a provider body, the way a real provider calls it. Not a FutureProvider:
/// the connection's reconnect backoff would rebuild that and the test would only ever see "loading".
final _probe = Provider.family<Future<Multiplexer>, MacDevice>(requireMux);

/// The connection to [_device] dials [dial]; both providers are held open so nothing is disposed mid-await.
ProviderContainer _container(Future<Transport> Function() dial) {
  final c = ProviderContainer(
    overrides: [
      macStorageProvider.overrideWithValue(DemoStorage([_device])),
      transportFactoryProvider.overrideWithValue((_, _) => dial()),
    ],
  );
  addTearDown(c.dispose);
  final conn = c.listen(connectionNotifierProvider(_device), (_, _) {});
  addTearDown(conn.close);
  return c;
}

Future<Object?> _error(ProviderContainer c) async {
  try {
    await c.read(_probe(_device));
  } on Object catch (e) {
    return e;
  }
  return null;
}

void main() {
  test('returns the live Multiplexer when the computer is connected', () async {
    final c = _container(() async => _Host(2));
    expect(await c.read(_probe(_device)), isA<Multiplexer>());
  });

  test(
    'a disconnected device throws DeviceOffline with the one message',
    () async {
      final c = _container(() async => throw Exception('refused'));
      final e = await _error(c);
      expect(e, isA<DeviceOffline>());
      expect((e! as DeviceOffline).device, _device);
      expect((e as DeviceOffline).message, 'Studio is offline');
      expect('$e', 'Studio is offline');
    },
  );

  test('a computer on another protocol throws DeviceProtocolMismatch, not DeviceOffline', () async {
    final c = _container(() async => _Host(1));
    final e = await _error(c);
    expect(e, isA<DeviceProtocolMismatch>());
    expect((e! as DeviceProtocolMismatch).device, _device);
    expect((e as DeviceProtocolMismatch).message, contains('Update Atlas'));
  });

  test('errorText words every failure once', () {
    expect(errorText(const DeviceOffline(_device)), 'Studio is offline');
    expect(errorText(StateError('the server refused')), 'the server refused');
    expect(errorText(Exception('boom')), 'Exception: boom');
  });
}
