import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/bots/bots_screen.dart';
import 'package:atlas_mobile/features/bots/bot_screen.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';

import 'package:atlas_mobile/core/demo/demo_computer.dart';

// ─── Helpers ─────────────────────────────────────────────────────────────────

class _Storage extends MacStorage {
  _Storage(this.devices) : super(storage: null);
  final List<MacDevice> devices;
  @override
  Future<List<MacDevice>> loadDevices() async => devices;
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

Widget _wrap(Widget child, List<MacDevice> devices) => ProviderScope(
  overrides: [
    macStorageProvider.overrideWithValue(_Storage(devices)),
    transportFactoryProvider.overrideWithValue((_, _) async => demoTransport()),
  ],
  child: MaterialApp(theme: atlasTheme(), home: child),
);

Future<void> _settle(WidgetTester t) async {
  for (var i = 0; i < 30; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

void main() {
  const mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

  // 1. Flat list order — Needs-you card above the main bot list
  testWidgets('flat list order and chips', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(_wrap(const BotsScreen(), [mac]));
    await _settle(t);

    // Every crewmate stands on the deck and has a logbook card; Nightly also asks for
    // approval, so it has a captain's-call card too.
    expect(find.text('Reviewer'), findsNWidgets(2));
    expect(find.text('Nightly'), findsNWidgets(3));

    // Needs-you card shows inline Deny/Allow.
    expect(find.text('Deny'), findsOneWidget);
    expect(find.text('Allow'), findsOneWidget);

    // A working crewmate says so in its row, Telegram's "typing…" style.
    expect(find.text('working…'), findsOneWidget);

    // Captain's call comes before the logbook: Deny is above Reviewer's logbook card.
    final denyY = t.getTopLeft(find.text('Deny')).dy;
    final reviewerCardY = t.getTopLeft(find.text('Reviewer').last).dy;
    expect(denyY, lessThan(reviewerCardY));

    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });

  // The demo showed an empty Atlas Crew (and Inbox): both read the paired computers
  // only. With nothing paired and the demo on, the demo's crewmates must be listed.
  testWidgets('demo: the crew tab lists the demo crewmates', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    final container = ProviderContainer(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage(const [])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => demoTransport(),
        ),
      ],
    );
    container.read(demoModeProvider.notifier).state = true;
    await t.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: MaterialApp(theme: atlasTheme(), home: const BotsScreen()),
      ),
    );
    await _settle(t);
    expect(find.text('Reviewer'), findsWidgets);
    expect(find.text('Nightly'), findsWidgets);
    await t.pumpWidget(const SizedBox());
    // The demo computer's timers live with the container: dispose it inside the test.
    container.dispose();
    await _settle(t);
  });

  // 2. Approve from the list — open Nightly's BotScreen, see approval bubble,
  //    tap Allow, banner goes away.
  testWidgets('approve from the list', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(_wrap(BotScreen(device: mac, botId: 'bot-2'), [mac]));
    await _settle(t);

    // Approval bubble shows Allow / Deny.
    expect(find.text('Allow'), findsOneWidget);
    expect(find.text('Deny'), findsOneWidget);

    // Tap Allow.
    await t.tap(find.text('Allow'));
    for (var i = 0; i < 15; i++) {
      await t.pump(const Duration(milliseconds: 100));
    }

    // After answering, the approval bubble is gone.
    expect(find.text('Allow'), findsNothing);

    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });

  // 3. Invite sheet — screen renders without crashing.
  testWidgets('invite sheet — screen renders', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(_wrap(const BotsScreen(), [mac]));
    await _settle(t);
    expect(find.byType(BotsScreen), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });

  // 4. Offline banner — BotScreen renders gracefully when connection is pending.
  testWidgets('offline banner in bot screen', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    final completer = Completer<Transport>();
    await t.pumpWidget(
      ProviderScope(
        overrides: [
          macStorageProvider.overrideWithValue(_Storage([mac])),
          transportFactoryProvider.overrideWithValue(
            (_, _) => completer.future,
          ),
        ],
        child: MaterialApp(
          theme: atlasTheme(),
          home: BotScreen(device: mac, botId: 'bot-1'),
        ),
      ),
    );
    for (var i = 0; i < 5; i++) {
      await t.pump(const Duration(milliseconds: 100));
    }
    expect(find.byType(BotScreen), findsOneWidget);
    completer.complete(demoTransport());
    await t.pumpWidget(const SizedBox());
    await t.pump();
  });

  // 5. HomeScreen renders with demo data.
  testWidgets('HomeScreen renders', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(_wrap(const HomeScreen(), [mac]));
    await _settle(t);
    expect(find.byType(HomeScreen), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });
}
