import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'fake_transport.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';

class _FakeStorage extends MacStorage {
  _FakeStorage() : super(storage: null);
  @override
  Future<String?> loadSecretKey() async => null;
  @override
  Future<List<MacDevice>> loadDevices() async => [];
  @override
  Future<void> saveDevices(List<MacDevice> _) async {}
  @override
  Future<void> saveSecretKey(String _) async {}
  @override
  Future<void> upsertDevice(MacDevice _) async {}
  @override
  Future<void> removeDevice(String _) async {}
}

// Reply injected for agent_status_all (id=1, the first mux call).
const _statusAllReply =
    '{"id":1,"ok":true,"data":{"panes":{},"pending_approvals":'
    '[{"pane_key":"p1","agent":"a","req":"r1","tool_name":"Bash","tool_input":"ls"}]}}';

void main() {
  final device = MacDevice(nodeId: 'test-node', name: 'Test Mac');

  group('PendingApprovalNotifier', () {
    late FakeTransport ft;
    late ProviderContainer container;

    setUp(() {
      ft = FakeTransport();
      container = ProviderContainer(
        overrides: [
          transportFactoryProvider.overrideWithValue((_, _) async => ft),
          macStorageProvider.overrideWithValue(_FakeStorage()),
        ],
      );
    });

    tearDown(() => container.dispose());

    /// Waits for the connection to come up, then yields one microtask so that
    /// [_seed] can proceed past its `await ref.read(connectionNotifier.future)`
    /// and register the `agent_status_all` call on the mux.
    Future<void> waitForMuxReady() async {
      await container.read(connectionNotifierProvider(device).future);
      await Future<void>.delayed(Duration.zero);
    }

    test('seeds the approval from agent_status_all', () async {
      container.listen(
        pendingApprovalNotifierProvider((device, 'p1')),
        (_, _) {},
        fireImmediately: true,
      );

      await waitForMuxReady();

      // id=1 is the first call — agent_status_all from _seed().
      ft.injectLine(_statusAllReply);

      await Future<void>.delayed(const Duration(milliseconds: 10));

      final state = container.read(
        pendingApprovalNotifierProvider((device, 'p1')),
      );
      expect(state, isNotNull);
      expect(state!.req, 'r1');
      expect(state.paneKey, 'p1');
    });

    test('clears approval on agent_approval_done push', () async {
      container.listen(
        pendingApprovalNotifierProvider((device, 'p1')),
        (_, _) {},
        fireImmediately: true,
      );

      await waitForMuxReady();

      // Seed initial approval.
      ft.injectLine(_statusAllReply);
      await Future<void>.delayed(const Duration(milliseconds: 10));

      expect(
        container.read(pendingApprovalNotifierProvider((device, 'p1'))),
        isNotNull,
        reason: 'precondition: approval seeded',
      );

      // Server pushes done — approval should clear.
      ft.injectLine(
        jsonEncode({
          'ev': 'agent_approval_done',
          'pane_key': 'p1',
          'req': 'r1',
        }),
      );
      await Future<void>.delayed(const Duration(milliseconds: 10));

      expect(
        container.read(pendingApprovalNotifierProvider((device, 'p1'))),
        isNull,
      );
    });

    test('agent_approval push while no seed sets the approval', () async {
      container.listen(
        pendingApprovalNotifierProvider((device, 'p1')),
        (_, _) {},
        fireImmediately: true,
      );

      await waitForMuxReady();

      // Push approval event before the seed reply arrives.
      ft.injectLine(
        jsonEncode({
          'ev': 'agent_approval',
          'pane_key': 'p1',
          'agent': 'a',
          'req': 'r1',
          'tool_name': 'Bash',
          'tool_input': 'ls',
        }),
      );
      await Future<void>.delayed(const Duration(milliseconds: 10));

      final state = container.read(
        pendingApprovalNotifierProvider((device, 'p1')),
      );
      expect(state, isNotNull);
      expect(state!.req, 'r1');

      // Now the seed reply arrives; _seed() sees state != null and bails out.
      ft.injectLine(_statusAllReply);
      await Future<void>.delayed(const Duration(milliseconds: 10));

      // State unchanged — push-set value is kept.
      expect(
        container.read(pendingApprovalNotifierProvider((device, 'p1'))),
        isNotNull,
      );
    });
  });
}
