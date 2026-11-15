// Regression (phone screenshots 2026-11-15): the first-run onboarding was the Home tab's content, so the shell's
// floating tab bar sat on top of it, covering its Continue button and offering tabs with nothing to show yet.
// Through the REAL shell (the app's own routes): with nothing paired, the onboarding is the whole screen, no bar,
// and every step is reachable; once a computer is paired the tabs come back.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/home/dashboard.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/home/onboarding.dart';
import 'package:atlas_mobile/main.dart';

import 'support/demo_harness.dart';

/// Storage whose paired list can change, like pairing does (it saves, then invalidates the provider).
class _Storage extends DemoStorage {
  _Storage(this.paired) : super([]);
  List<MacDevice> paired;
  @override
  Future<List<MacDevice>> loadDevices() async => paired;
}

const _mac = MacDevice(nodeId: 'mac-1', name: 'My Mac');

Future<_Storage> _launch(
  WidgetTester t, {
  bool seen = false,
  List<MacDevice> paired = const [],
}) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  final storage = _Storage([...paired]);
  final router = buildRouter();
  addTearDown(router.dispose);
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(storage),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => demoTransport(),
        ),
        if (seen) onboardingSeenProvider.overrideWith((_) => true),
      ],
      child: MaterialApp.router(theme: atlasTheme(), routerConfig: router),
    ),
  );
  await settle(t);
  return storage;
}

/// The tab bar is on screen (offstage widgets are skipped by default finders).
bool _barShown(WidgetTester t) =>
    find.byType(FloatingTabBar).evaluate().isNotEmpty;

ProviderContainer _container(WidgetTester t) =>
    ProviderScope.containerOf(t.element(find.byType(MaterialApp)));

void main() {
  testWidgets(
    'with nothing paired the onboarding is the whole screen, no tab bar',
    (t) async {
      await _launch(t);

      expect(find.byType(Onboarding), findsOneWidget);
      expect(
        _barShown(t),
        isFalse,
        reason: 'the tab bar must not float over the onboarding',
      );

      // Every page's primary button is on screen and receives the tap (it was half under the bar).
      for (var i = 0; i < 3; i++) {
        final button = find.widgetWithText(FilledButton, 'Continue');
        expect(t.getRect(button).bottom, lessThanOrEqualTo(852));
        expect(button.hitTestable(), findsOneWidget);
        await t.tap(button);
        await settle(t);
        expect(_barShown(t), isFalse);
      }
      expect(
        find.widgetWithText(FilledButton, 'Scan pairing code'),
        findsOneWidget,
      );
    },
  );

  testWidgets('pairing a computer replaces the onboarding with the tabs', (
    t,
  ) async {
    final storage = await _launch(t, seen: true);
    expect(find.byType(Onboarding), findsOneWidget);

    storage.paired = [_mac];
    _container(t).invalidate(pairedDevicesProvider);
    await settle(t);

    expect(find.byType(Onboarding), findsNothing);
    expect(_barShown(t), isTrue);
    expect(find.byType(HomeDashboard), findsOneWidget);
  });

  // Debug builds show the onboarding on every launch; with a computer paired, its last button opens that
  // computer (no pairing, no demo).
  testWidgets('paired (debug, every launch): the last page opens my computer', (
    t,
  ) async {
    await _launch(t, paired: [_mac]);
    expect(find.byType(Onboarding), findsOneWidget);
    expect(_barShown(t), isFalse);
    for (var i = 0; i < 3; i++) {
      await t.tap(find.widgetWithText(FilledButton, 'Continue'));
      await settle(t);
    }
    await t.tap(find.widgetWithText(FilledButton, 'Open my computer'));
    await settle(t);
    expect(find.byType(Onboarding), findsNothing);
    expect(_barShown(t), isTrue);
    expect(find.text('My Mac'), findsWidgets);
  });

  // App Review reaches the app through "Explore a demo": the demo shows the tabs; leaving it with nothing
  // paired is the first run again, not an empty Home under a tab bar.
  testWidgets(
    'leaving the demo with nothing paired returns to the onboarding',
    (t) async {
      await _launch(t);
      _container(t).read(demoModeProvider.notifier).state = true;
      await settle(t);
      expect(find.byType(Onboarding), findsNothing);
      expect(_barShown(t), isTrue);
      expect(find.text('orbit-shop'), findsWidgets);

      _container(t).read(demoModeProvider.notifier).state = false;
      await settle(t);
      expect(find.byType(Onboarding), findsOneWidget);
      expect(_barShown(t), isFalse);
    },
  );
}
