import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';

import 'package:atlas_mobile/core/demo/demo_computer.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';

class _Storage extends MacStorage {
  _Storage(this.devices) : super(storage: null);
  final List<MacDevice> devices;
  @override
  Future<List<MacDevice>> loadDevices() async => devices;
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

Future<void> _settle(WidgetTester t) async {
  for (var i = 0; i < 30; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
}

Future<void> _app(
  WidgetTester t,
  List<MacDevice> paired, {
  Duration connectDelay = Duration.zero,
  List<Map<String, String>> integrations = const [],
  bool settled = true,
}) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage(paired)),
        // The real computer is never reached in tests; the demo protocol stands in.
        transportFactoryProvider.overrideWithValue((_, _) async {
          await Future<void>.delayed(connectDelay);
          return demoTransport(integrations: integrations);
        }),
      ],
      child: MaterialApp(theme: atlasTheme(), home: const HomeScreen()),
    ),
  );
  if (settled) await _settle(t);
}

void main() {
  const mac = MacDevice(nodeId: 'n1', name: 'My Mac');

  // Reported on device: launch showed a logo, then a second "Reaching…" screen with its
  // own logo. One loading screen from launch to data, then the dashboard.
  testWidgets('one loading screen while the computer connects', (t) async {
    await _app(
      t,
      [mac],
      connectDelay: const Duration(seconds: 2),
      settled: false,
    );
    for (var i = 0; i < 8; i++) {
      await t.pump(const Duration(milliseconds: 100));
      expect(find.byType(AtlasLogo), findsOneWidget);
      expect(find.text('Projects'), findsNothing);
    }
    expect(
      find.textContaining(RegExp('Reaching My Mac…|Loading projects…')),
      findsOneWidget,
    );
    for (var i = 0; i < 40; i++) {
      await t.pump(const Duration(milliseconds: 100));
    }
    expect(find.text('Projects'), findsOneWidget);
    expect(
      find.byType(AtlasLogo),
      findsOneWidget,
      reason: 'only the header mark',
    );
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });

  // An agent whose hooks are off never shows on the phone, with no hint why: the home
  // screen names each installed agent whose integration is off, and only those.
  testWidgets('home names agents whose integration is off', (t) async {
    await _app(
      t,
      [mac],
      integrations: [
        {'id': 'claude', 'label': 'Claude Code', 'state': 'missing'},
        {'id': 'omp', 'label': 'omp', 'state': 'ok'},
        {'id': 'codex', 'label': 'Codex', 'state': 'unknown'},
      ],
    );
    expect(find.text('Claude Code integration is off'), findsOneWidget);
    expect(find.textContaining('integration is off'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });

  // App Review reaches the app only through "Explore a demo": with nothing paired and
  // the real transport factory (no override), the demo computer must answer in the app,
  // never be dialled. (Leaving it returns to the first run: test/first_run_test.dart.)
  testWidgets('demo: the in-app computer opens with nothing paired', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    final container = ProviderContainer(
      overrides: [macStorageProvider.overrideWithValue(_Storage(const []))],
    );
    addTearDown(container.dispose);
    container.read(demoModeProvider.notifier).state = true;
    await t.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: MaterialApp(theme: atlasTheme(), home: const HomeScreen()),
      ),
    );
    await _settle(t);
    expect(find.text('orbit-shop'), findsWidgets);
    // Leaving stops the demo computer's timers before the tree goes away.
    container.read(demoModeProvider.notifier).state = false;
    await _settle(t);
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });
}
