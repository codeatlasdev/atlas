import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/push/push_payload.dart';
import 'package:atlas_mobile/core/push/push_service.dart';
import 'package:atlas_mobile/core/push/push_tap.dart';
import 'package:atlas_mobile/main.dart';

import 'support/demo_harness.dart';
import 'support/fake_permissions.dart';

// Launching the app never asks for notifications (it used to, before anyone had seen a screen). The token is
// fetched, and so registered with the computers, only once they are allowed.

class _Push implements PushBackend {
  _Push({this.configured = true, this.launch});
  final bool configured;
  final Map<String, dynamic>? launch;
  var tokenCalls = 0;
  final _opened = StreamController<Map<String, dynamic>>.broadcast();

  @override
  Future<bool> start() async => configured;
  @override
  Future<String?> token() async {
    tokenCalls++;
    return 'fcm-1';
  }

  @override
  Stream<String> get tokenRefresh => const Stream.empty();
  @override
  Stream<Map<String, dynamic>> get opened => _opened.stream;
  @override
  Future<Map<String, dynamic>?> launchedBy() async => launch;
}

/// Records what the tap on a push put in [pushTapProvider] (the shell consumes it at once).
base class _Tap extends ProviderObserver {
  _Tap(this.seen);
  final List<Object?> seen;
  @override
  void didUpdateProvider(
    ProviderObserverContext context,
    Object? previousValue,
    Object? newValue,
  ) {
    if (context.provider == pushTapProvider) seen.add(newValue);
  }
}

const _n = AtlasPermission.notifications;

Future<ProviderContainer> _launch(
  WidgetTester t,
  FakePermissions perms,
  _Push push, {
  List<ProviderObserver> observers = const [],
}) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  await t.pumpWidget(
    ProviderScope(
      observers: observers,
      overrides: [
        permissionsProvider.overrideWithValue(perms),
        pushBackendProvider.overrideWithValue(push),
        macStorageProvider.overrideWithValue(DemoStorage([])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => demoTransport(),
        ),
      ],
      child: const AtlasMobileApp(),
    ),
  );
  await settle(t);
  return ProviderScope.containerOf(t.element(find.byType(MaterialApp)));
}

void main() {
  testWidgets(
    'launch with notifications not asked: nothing requested, no token',
    (t) async {
      final perms = FakePermissions(states: {_n: PermissionState.notAsked});
      final push = _Push();
      final c = await _launch(t, perms, push);

      expect(perms.requested, isEmpty);
      expect(push.tokenCalls, 0);
      expect(c.read(fcmTokenProvider), isNull);
    },
  );

  testWidgets(
    'launch with notifications refused in Settings: still nothing asked',
    (t) async {
      final perms = FakePermissions(states: {_n: PermissionState.blocked});
      final push = _Push();
      final c = await _launch(t, perms, push);

      expect(perms.requested, isEmpty);
      expect(push.tokenCalls, 0);
      expect(c.read(fcmTokenProvider), isNull);
    },
  );

  testWidgets('launch with notifications allowed: the token is published', (
    t,
  ) async {
    final perms = FakePermissions(states: {_n: PermissionState.granted});
    final push = _Push();
    final c = await _launch(t, perms, push);

    expect(perms.requested, isEmpty);
    expect(c.read(fcmTokenProvider), 'fcm-1');
  });

  testWidgets(
    'allowed later (after pairing, or in Settings): the token follows',
    (t) async {
      final perms = FakePermissions(
        states: {_n: PermissionState.notAsked},
        answers: {_n: PermissionState.granted},
      );
      final push = _Push();
      final c = await _launch(t, perms, push);
      expect(c.read(fcmTokenProvider), isNull);

      await perms.request(_n); // what the prompt does, after the explanation
      c.invalidate(permissionStateProvider(_n));
      await settle(t);

      expect(c.read(fcmTokenProvider), 'fcm-1');
    },
  );

  testWidgets(
    'allowed in the system Settings while the app was away: read again on return',
    (t) async {
      final perms = FakePermissions(states: {_n: PermissionState.blocked});
      final push = _Push();
      final c = await _launch(t, perms, push);
      expect(c.read(fcmTokenProvider), isNull);

      perms.states[_n] = PermissionState.granted;
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await settle(t);

      expect(c.read(fcmTokenProvider), 'fcm-1');
    },
  );

  testWidgets('no Firebase config (a local build): nothing happens', (t) async {
    final perms = FakePermissions(states: {_n: PermissionState.granted});
    final push = _Push(configured: false);
    final c = await _launch(t, perms, push);

    expect(push.tokenCalls, 0);
    expect(c.read(fcmTokenProvider), isNull);
    expect(perms.requested, isEmpty);
  });

  testWidgets('the push that launched the app opens what it is about', (
    t,
  ) async {
    final perms = FakePermissions(states: {_n: PermissionState.granted});
    final push = _Push(
      launch: {'kind': 'agent_done', 'pane': 'p1', 'title': 'Claude · atlas'},
    );
    final taps = <Object?>[];
    await _launch(t, perms, push, observers: [_Tap(taps)]);

    expect(taps.whereType<PushPayload>().map((p) => p.pane), ['p1']);
  });
}
