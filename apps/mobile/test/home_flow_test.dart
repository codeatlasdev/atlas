import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'fixtures/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/home/onboarding.dart';

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
}) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage(paired)),
        // The real computer is never reached in tests; the demo protocol stands in.
        transportFactoryProvider.overrideWithValue(
          (_, _) async => demoTransport(),
        ),
      ],
      child: MaterialApp(
        theme: atlasTheme(),
        home: const HomeScreen(),
      ),
    ),
  );
  await _settle(t);
}

Future<void> _toLastPage(WidgetTester t) async {
  for (var i = 0; i < 3; i++) {
    await t.tap(find.text('Continue'));
    await _settle(t);
  }
}

void main() {
  const mac = MacDevice(nodeId: 'n1', name: 'My Mac');

  // Reported on device: launch showed a logo, then a second "Reaching…" screen with its
  // own logo. One loading screen from launch to data, then the dashboard.
  testWidgets('one loading screen while the computer connects', (t) async {
    await _app(t, [mac], connectDelay: const Duration(seconds: 2));
    await _toLastPage(t);
    await t.tap(find.text('Open my computer'));
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

  // Debug builds show the onboarding on every launch; with a computer paired, its last
  // button opens that computer, and the onboarding offers no demo.
  testWidgets('paired: onboarding opens my computer', (t) async {
    await _app(t, [mac]);
    expect(find.byType(Onboarding), findsOneWidget);
    expect(find.text('Try the demo'), findsNothing);
    await _toLastPage(t);
    await t.tap(find.text('Open my computer'));
    await _settle(t);
    expect(find.byType(Onboarding), findsNothing);
    expect(find.text('My Mac'), findsWidgets);
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });
}
