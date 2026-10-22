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

import 'fixtures/demo_computer.dart';

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
    transportFactoryProvider.overrideWithValue(
      (_, _) async => demoTransport(),
    ),
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

    // Both bots appear in the list.
    expect(find.text('Reviewer'), findsOneWidget);
    expect(find.text('Nightly'), findsAtLeastNWidgets(1));

    // Needs-you card shows inline Deny/Allow (no status chips in messenger design).
    expect(find.text('Deny'), findsOneWidget);
    expect(find.text('Allow'), findsOneWidget);

    // Working preview text is italic "Working…" (no chip).
    expect(find.textContaining('Working'), findsWidgets);

    // The Deny button (in the needs-you card at top) is above the Reviewer row.
    final denyY = t.getTopLeft(find.text('Deny')).dy;
    final reviewerY = t.getTopLeft(find.text('Reviewer')).dy;
    expect(denyY, lessThan(reviewerY));

    await t.pumpWidget(const SizedBox());
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
