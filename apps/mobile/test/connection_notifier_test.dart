import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/transport/fake_transport.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';

// ── Fake MacStorage ───────────────────────────────────────────────────────────
// Extends MacStorage but overrides every method so FlutterSecureStorage
// platform channels are never invoked during tests.

class _FakeMacStorage extends MacStorage {
  _FakeMacStorage() : super(storage: null);

  @override
  Future<List<MacDevice>> loadDevices() async => [];

  @override
  Future<String?> loadSecretKey() async => null;

  @override
  Future<void> saveDevices(List<MacDevice> devices) async {}

  @override
  Future<void> saveSecretKey(String hexKey) async {}

  @override
  Future<void> upsertDevice(MacDevice device) async {}

  @override
  Future<void> removeDevice(String nodeId) async {}

  @override
  Future<MacDevice?> getDevice(String nodeId) async => null;
}

void main() {
  // MacDevice is a freezed class — factory constructor, not const.
  final device = MacDevice(nodeId: 'test-node', name: 'Test Mac');

  group('ConnectionNotifier', () {
    test('connects successfully when transport factory succeeds', () async {
      final fakeTransport = FakeTransport();

      final container = ProviderContainer(overrides: [
        transportFactoryProvider.overrideWithValue(
          (_, _) async => fakeTransport,
        ),
        macStorageProvider.overrideWithValue(_FakeMacStorage()),
      ]);
      addTearDown(container.dispose);

      final mux =
          await container.read(connectionNotifierProvider(device).future);
      expect(mux, isNotNull);
      expect(mux, isA<Multiplexer>());
    });

    test('returns null (and schedules reconnect) when factory throws', () async {
      final container = ProviderContainer(overrides: [
        transportFactoryProvider.overrideWithValue(
          (_, _) async => throw Exception('connection refused'),
        ),
        macStorageProvider.overrideWithValue(_FakeMacStorage()),
      ]);
      addTearDown(container.dispose);

      // _connect() catches errors and returns null; provider resolves to null.
      final mux =
          await container.read(connectionNotifierProvider(device).future);
      expect(mux, isNull);
    });

    test('transitions through loading then delivers mux', () async {
      final fakeTransport = FakeTransport();
      final container = ProviderContainer(overrides: [
        transportFactoryProvider.overrideWithValue(
          (_, _) async => fakeTransport,
        ),
        macStorageProvider.overrideWithValue(_FakeMacStorage()),
      ]);
      addTearDown(container.dispose);

      final states = <AsyncValue<Multiplexer?>>[];
      final sub = container.listen(
        connectionNotifierProvider(device),
        (_, next) => states.add(next),
        fireImmediately: true,
      );

      await container.read(connectionNotifierProvider(device).future);
      sub.close();

      // At some point the state must have been loading.
      expect(states.any((s) => s.isLoading), isTrue);
      // Final state is data with a non-null mux.
      expect(states.last.hasValue, isTrue);
      expect(states.last.value, isNotNull);
    });
  });
}
