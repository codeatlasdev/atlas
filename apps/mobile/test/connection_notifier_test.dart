import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'fake_transport.dart';
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

}

void main() {
  // MacDevice uses a const constructor with equality on nodeId.
  final device = MacDevice(nodeId: 'test-node', name: 'Test Mac');

  group('ConnectionNotifier', () {
    test('connects successfully when transport factory succeeds', () async {
      final fakeTransport = FakeTransport();

      final container = ProviderContainer(
        overrides: [
          transportFactoryProvider.overrideWithValue(
            (_, _) async => fakeTransport,
          ),
          macStorageProvider.overrideWithValue(_FakeMacStorage()),
        ],
      );
      addTearDown(container.dispose);

      final mux = await container.read(
        connectionNotifierProvider(device).future,
      );
      expect(mux, isNotNull);
      expect(mux, isA<Multiplexer>());
    });

    test('returns null (and schedules reconnect) when factory throws', () async {
      final container = ProviderContainer(
        overrides: [
          transportFactoryProvider.overrideWithValue(
            (_, _) async => throw Exception('connection refused'),
          ),
          macStorageProvider.overrideWithValue(_FakeMacStorage()),
        ],
      );
      addTearDown(container.dispose);

      // _connect() catches errors and returns null; provider resolves to null.
      final mux = await container.read(
        connectionNotifierProvider(device).future,
      );
      expect(mux, isNull);
    });

    test('transitions through loading then delivers mux', () async {
      final fakeTransport = FakeTransport();
      final container = ProviderContainer(
        overrides: [
          transportFactoryProvider.overrideWithValue(
            (_, _) async => fakeTransport,
          ),
          macStorageProvider.overrideWithValue(_FakeMacStorage()),
        ],
      );
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

    // Guards the ~1 min first load: after the app sat idle or in the background,
    // the connection could be dead without closing; the first request waited out
    // its full deadline before anything reconnected.
    test('coming back to the foreground replaces a connection that does not answer',
        () async {
      final dead = FakeTransport(), fresh = FakeTransport();
      var made = 0;
      final container = ProviderContainer(
        overrides: [
          transportFactoryProvider.overrideWithValue(
            (_, _) async => [dead, fresh][made++],
          ),
          macStorageProvider.overrideWithValue(_FakeMacStorage()),
        ],
      );
      addTearDown(container.dispose);
      final p = connectionNotifierProvider(device);
      final sub = container.listen(p, (_, _) {});
      addTearDown(sub.close);
      final first = await container.read(p.future);
      final n = container.read(p.notifier)..probeTimeout = const Duration(milliseconds: 50);
      final sw = Stopwatch()..start();
      await n.onForegrounded();
      final second = await container.read(p.future);
      expect(made, 2, reason: 'a silent connection is replaced');
      expect(identical(first, second), isFalse);
      expect(sw.elapsed, lessThan(const Duration(seconds: 2)));
      expect(dead.sentLines.single, contains('"cmd":"info"'));
    });

    test('a connection that answers the probe is kept', () async {
      final live = FakeTransport();
      var made = 0;
      final container = ProviderContainer(
        overrides: [
          transportFactoryProvider.overrideWithValue((_, _) async {
            made++;
            return live;
          }),
          macStorageProvider.overrideWithValue(_FakeMacStorage()),
        ],
      );
      addTearDown(container.dispose);
      final p = connectionNotifierProvider(device);
      final sub = container.listen(p, (_, _) {});
      addTearDown(sub.close);
      await container.read(p.future);
      final probing = container.read(p.notifier).onForegrounded();
      await Future<void>.delayed(Duration.zero);
      live.injectLine('{"id":1,"ok":true,"data":{}}');
      await probing;
      expect(made, 1);
    });

    // Guards Minor 5: a reconnect timer that was scheduled before the app was
    // backgrounded must not fire and open a new connection in the background.
    test('backgrounding cancels the reconnect timer', () async {
      final fakeTransport = FakeTransport();
      var made = 0;
      final container = ProviderContainer(
        overrides: [
          transportFactoryProvider.overrideWithValue((_, _) async {
            made++;
            return fakeTransport;
          }),
          macStorageProvider.overrideWithValue(_FakeMacStorage()),
        ],
      );
      addTearDown(container.dispose);
      final p = connectionNotifierProvider(device);
      final sub = container.listen(p, (_, _) {});
      addTearDown(sub.close);
      await container.read(p.future);
      expect(made, 1);
      // Background the notifier: cancels any pending reconnect timer and sets
      // _foregrounded = false so _scheduleReconnect() returns early.
      container.read(p.notifier).onBackgrounded();
      // Simulate the transport closing (network drop, device sleep, etc.).
      await fakeTransport.simulateRemoteClose();
      // Give any erroneous timer a chance to fire.
      await Future<void>.delayed(const Duration(milliseconds: 100));
      expect(made, 1, reason: 'backgrounded: no reconnect must be attempted');
    });

    // Guards Minor 6: a probe reply of {"ok":false,"error":"forbidden"} must
    // count as proof the link is alive (the id field is present), not trigger
    // a reconnect.
    test('a forbidden probe reply keeps the connection', () async {
      final live = FakeTransport();
      var made = 0;
      final container = ProviderContainer(
        overrides: [
          transportFactoryProvider.overrideWithValue((_, _) async {
            made++;
            return live;
          }),
          macStorageProvider.overrideWithValue(_FakeMacStorage()),
        ],
      );
      addTearDown(container.dispose);
      final p = connectionNotifierProvider(device);
      final sub = container.listen(p, (_, _) {});
      addTearDown(sub.close);
      await container.read(p.future);
      final probing = container.read(p.notifier).onForegrounded();
      await Future<void>.delayed(Duration.zero);
      // "forbidden" is a real reply: the id field is present, so the link is alive.
      live.injectLine('{"id":1,"ok":false,"error":"forbidden"}');
      await probing;
      expect(made, 1, reason: 'forbidden counts as alive; no reconnect');
    });
  });
}
