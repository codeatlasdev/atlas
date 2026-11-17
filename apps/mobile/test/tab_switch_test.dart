import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_provider.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';
import 'package:atlas_mobile/features/home/dashboard.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/home/onboarding.dart';
import 'package:atlas_mobile/features/home/splash.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/settings/settings_screen.dart';
import 'package:atlas_mobile/main.dart';

import 'support/demo_harness.dart';
import 'support/frame_probe.dart';
import 'support/fake_permissions.dart';

// Regression (screen recording 2026-11-12, tab_overlap_67s): switching tabs (Settings -> Projects) showed both pages
// superimposed, because the shell cross-faded the page being left with the page being entered for 260 ms.
// Frame by frame, through the REAL shell (the app's own routes, real providers, the demo computer as transport):
// one page at any frame, each tab keeps its scroll, Settings opens without anything moving, Reduce motion cuts.

const _mac = MacDevice(nodeId: 'mac-1', name: 'Mac');

/// Paired devices that load when the test says so (the first frames after launch, before storage answered).
class _GatedStorage extends DemoStorage {
  _GatedStorage(super.devices, this.gate);
  final Future<void> gate;
  @override
  Future<List<MacDevice>> loadDevices() async {
    await gate;
    return devices;
  }
}

enum _Tab { home, crew, inbox, settings }

final _page = <_Tab, Finder>{
  _Tab.home: find.byType(HomeScreen, skipOffstage: false),
  _Tab.crew: find.byType(CrewScreen, skipOffstage: false),
  _Tab.inbox: find.byType(InboxScreen, skipOffstage: false),
  _Tab.settings: find.byType(SettingsScreen, skipOffstage: false),
};

Finder _barItem(_Tab tab) => switch (tab) {
  _Tab.home => find.byType(CodeXmlIcon),
  _Tab.crew => find.byType(ShipFlagIcon),
  _Tab.inbox => find.byIcon(Icons.notifications_rounded),
  _Tab.settings => find.byIcon(Icons.settings_rounded),
};

/// The app's real routes and shell on a phone-sized screen. [reduced]: Reduce motion is on. [storage]: the paired
/// computers' storage (default: one computer, answered at once).
Future<void> _launch(
  WidgetTester t, {
  bool reduced = false,
  double height = 852,
  MacStorage? storage,
  List<Override> overrides = const [],
}) async {
  await t.binding.setSurfaceSize(Size(393, height));
  final router = buildRouter();
  addTearDown(router.dispose);
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(storage ?? DemoStorage([_mac])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => demoTransport(),
        ),
        // Debug builds show the onboarding on every launch; a release build opens the dashboard.
        onboardingSeenProvider.overrideWith((_) => true),
        ...overrides,
      ],
      child: MaterialApp.router(
        theme: atlasTheme(),
        routerConfig: router,
        builder: (context, child) => MediaQuery(
          data: MediaQuery.of(context).copyWith(disableAnimations: reduced),
          child: child!,
        ),
      ),
    ),
  );
}

/// Taps [tab] on the bar and lets the switch and whatever the page loads finish.
Future<void> _visit(WidgetTester t, _Tab tab) async {
  await t.tap(_barItem(tab));
  await settle(t);
}

FrameProbe _probe(WidgetTester t, [Map<String, Finder> extra = const {}]) =>
    FrameProbe(t, {
      for (final e in _page.entries) e.key.name: e.value,
      ...extra,
    });

void main() {
  for (final from in _Tab.values) {
    for (final to in _Tab.values) {
      if (from == to) continue;
      testWidgets(
        'switching ${from.name} -> ${to.name} never shows two pages at once',
        (t) async {
          await _launch(t);
          await settle(t);
          await _visit(t, from);

          final probe = _probe(t);
          await t.tap(_barItem(to));
          await probe.record(const Duration(milliseconds: 400));

          probe.expectNeverBothVisible(from.name, to.name);
          // Exactly one page on every frame (the outgoing one is gone on the first frame; the incoming one may
          // fade in over the ground, so it can be transparent on the first frames, never both).
          expect(
            probe.visibleFrames(from.name),
            isEmpty,
            reason: '${from.name} outstayed the switch',
          );
          final firstShown = probe.visibleFrames(to.name).first;
          expect(
            firstShown,
            lessThanOrEqualTo(1),
            reason:
                '${to.name} first showed on frame $firstShown (a blank screen between two pages)',
          );
          // The fade, when there is one, is short: fully opaque by 150 ms (frame 10 at 16 ms).
          final frames = probe.frames;
          expect(
            frames[10][to.name]!.opacity,
            1.0,
            reason: '${to.name} still fading after 150 ms',
          );
          for (final other in _Tab.values.where((o) => o != to)) {
            expect(
              frames.last[other.name]!.opacity,
              0.0,
              reason: '${other.name} still visible at the end',
            );
          }
        },
      );
    }
  }

  // Regression (same recording): the tab being left is not the only thing that must keep its place: coming
  // back finds each tab where it was.
  testWidgets('each tab keeps its scroll position after leaving and returning', (
    t,
  ) async {
    // Short screen so Settings and Computers scroll.
    await _launch(t, height: 360);
    await settle(t);
    await t.pump(const Duration(seconds: 2));

    double offset(_Tab tab) => t
        .state<ScrollableState>(
          find
              .descendant(
                of: _page[tab]!,
                matching: find.byType(Scrollable, skipOffstage: false),
                skipOffstage: false,
              )
              .first,
        )
        .position
        .pixels;

    for (final tab in [_Tab.home, _Tab.settings]) {
      await _visit(t, tab);
      await t.drag(
        find
            .descendant(of: _page[tab]!, matching: find.byType(Scrollable))
            .first,
        const Offset(0, -120),
      );
      await t.pump();
      expect(
        offset(tab),
        greaterThan(0),
        reason:
            '${tab.name} did not scroll in this test; make the screen shorter',
      );
    }
    final before = {
      for (final tab in [_Tab.home, _Tab.settings]) tab: offset(tab),
    };

    for (final away in [_Tab.crew, _Tab.inbox]) {
      await _visit(t, away);
    }
    for (final tab in [_Tab.home, _Tab.settings]) {
      final probe = _probe(t);
      await t.tap(_barItem(tab));
      await probe.record(
        const Duration(milliseconds: 400),
        sample: {'offset': () => offset(tab)},
      );
      probe.expectSampleWithin('offset', before[tab]!, before[tab]!);
    }
  });

  group('Settings opens without anything moving', () {
    // The bar is above the page; track the page's own parts.
    final parts = {
      'title': find.descendant(
        of: find.byType(SettingsScreen),
        matching: find.text('Settings'),
      ),
      'computers': find.descendant(
        of: find.byType(SettingsScreen),
        matching: find.text('Computers'),
      ),
      // The paired computer's row (its name).
      'card': find.descendant(
        of: find.byType(SettingsScreen),
        matching: find.text('Mac'),
      ),
      'pair': find.text('Pair another computer'),
      'privacy': find.text('Privacy policy'),
    };

    void noShift(FrameProbe p) {
      for (final name in parts.keys) {
        p.expectNoShift(name);
      }
    }

    testWidgets('from another tab', (t) async {
      await _launch(t);
      await settle(t);
      await _visit(t, _Tab.crew);

      final probe = _probe(t, parts);
      await t.tap(_barItem(_Tab.settings));
      await probe.record(const Duration(milliseconds: 600));
      noShift(probe);
      // The computer is listed on the very first frame the page is visible.
      final shown = probe.visibleFrames('settings').first;
      expect(
        probe.frames[shown]['card']!.rect,
        isNotNull,
        reason: 'the paired computer was not there when the page first showed',
      );
    });

    testWidgets(
      'the very first thing after launch, before the paired computers have loaded',
      (t) async {
        final gate = Completer<void>();
        await _launch(t, storage: _GatedStorage([_mac], gate.future));
        await t.pump();

        final probe = _probe(t, parts);
        await t.tap(_barItem(_Tab.settings));
        await probe.record(const Duration(milliseconds: 100));
        gate.complete(); // storage answers while Settings is on screen
        await probe.record(const Duration(milliseconds: 600));
        noShift(probe);
      },
    );

    testWidgets('while the notifications status arrives', (t) async {
      final status = Completer<String?>();
      await _launch(
        t,
        overrides: [
          pushRegistrationProvider.overrideWith((_) => status.future),
          // Only notifications exist on this phone, and the row arrives with the answer.
          permissionsProvider.overrideWithValue(
            FakePermissions(
              states: {
                AtlasPermission.camera: PermissionState.unavailable,
                AtlasPermission.localNetwork: PermissionState.unavailable,
              },
            ),
          ),
          permissionStateProvider(AtlasPermission.notifications).overrideWith(
            (ref) => status.future.then((_) => PermissionState.granted),
          ),
        ],
      );
      await settle(t);
      await _visit(t, _Tab.settings);

      final probe = _probe(t, {
        ...parts,
        'notifications': find.text('Notifications'),
      });
      await probe.record(const Duration(milliseconds: 100));
      status.complete('Notifications are on');
      await probe.record(const Duration(milliseconds: 600));
      // Nothing the user reads above the new row moves; what is below it slides open, never jumps a whole row.
      for (final name in ['title', 'computers', 'card', 'pair']) {
        probe.expectNoShift(name);
      }
      final tops = [
        for (final f in probe.frames)
          if (f['privacy']!.rect != null) f['privacy']!.rect!.top,
      ];
      final steps = [
        for (var k = 1; k < tops.length; k++) tops[k] - tops[k - 1],
      ];
      expect(
        steps.reduce((a, b) => a > b ? a : b),
        lessThan(40),
        reason: 'Privacy policy jumped by a whole row in one frame: $steps',
      );
      expect(
        tops.last - tops.first,
        greaterThan(40),
        reason: 'the Notifications row never opened',
      );
    });
  });

  testWidgets('Reduce motion switches tabs on one frame', (t) async {
    await _launch(t, reduced: true);
    await settle(t);
    await _visit(t, _Tab.settings);

    final probe = _probe(t);
    await t.tap(_barItem(_Tab.home));
    await probe.record(const Duration(milliseconds: 100));
    probe.expectNeverBothVisible('settings', 'home');
    expect(
      probe.frames.first['home']!.opacity,
      1.0,
      reason: 'the incoming page was not opaque on the first frame',
    );
    expect(
      probe.frames.first['settings']!.opacity,
      0.0,
      reason: 'the outgoing page was still showing on the first frame',
    );
  });

  // The first frames after launch: the loading screen, then the home, never the home's pieces over the
  // loading screen and never another tab.
  testWidgets('launch shows the loading screen, then the home, one at a time', (
    t,
  ) async {
    await _launch(t);
    final probe = FrameProbe(t, {
      for (final e in _page.entries) e.key.name: e.value,
      'splash': find.byType(HomeSplash),
      'dashboard': find.byType(HomeDashboard),
      'onboarding': find.byType(Onboarding),
    });
    await probe.record(const Duration(milliseconds: 3000));

    expect(
      probe.visibleFrames('splash').first,
      0,
      reason: 'the first frame was not the loading screen',
    );
    expect(
      probe.visibleFrames('dashboard'),
      isNotEmpty,
      reason: 'the home never showed',
    );
    probe.expectNeverBothVisible('splash', 'dashboard');
    // The home fades in over the ground in 150 ms at most.
    final shown = probe.visibleFrames('dashboard').first;
    expect(
      probe.frames[shown + 10]['dashboard']!.opacity,
      1.0,
      reason: 'the home was still fading 150 ms after it showed',
    );
    for (final other in ['crew', 'inbox', 'settings']) {
      expect(
        probe.visibleFrames(other),
        isEmpty,
        reason: '$other showed during launch',
      );
    }
  });
}
