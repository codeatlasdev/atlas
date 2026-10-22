import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/bots/bots_screen.dart';
import 'package:atlas_mobile/features/bots/bot_screen.dart';

import 'fixtures/demo_computer.dart';

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

Widget _wrap(Widget child, List<MacDevice> devices) => ProviderScope(
  overrides: [
    macStorageProvider.overrideWithValue(_Storage(devices)),
    transportFactoryProvider.overrideWithValue(
      (_, _) async => demoTransport(),
    ),
  ],
  child: MaterialApp(theme: atlasTheme(), home: child),
);

void main() {
  const mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

  testWidgets('Bots tab lists demo bot', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(_wrap(const BotsScreen(), [mac]));
    await _settle(t);
    // The demo computer has a "Reviewer" bot.
    expect(find.text('Reviewer'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });

  testWidgets('Bots tab empty state without paired computers', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(_wrap(const BotsScreen(), []));
    await _settle(t);
    // No devices paired → the screen says to pair a computer first.
    expect(find.textContaining('No bots yet'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });

  testWidgets('BotScreen: send task shows queued then done', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(
      _wrap(BotScreen(device: mac, botId: 'bot-1'), [mac]),
    );
    await _settle(t);
    // Enter a prompt and send.
    final field = find.byType(TextField);
    await t.enterText(field, 'Review this PR');
    await t.pump();
    // Tap send button.
    await t.tap(find.byIcon(Icons.arrow_upward_rounded));
    await _settle(t);
    // Task prompt is visible in the task list.
    expect(find.text('Review this PR'), findsOneWidget);
    // Wait for demo task to complete (~1.8 s total).
    for (var i = 0; i < 25; i++) {
      await t.pump(const Duration(milliseconds: 100));
    }
    // The demo result contains "demo".
    expect(find.textContaining('demo'), findsWidgets);
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });

  testWidgets('Bots tab shows bot needing approval in inbox', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    // demoDevice connects and demo computer returns a bot with pending_approval.
    await t.pumpWidget(_wrap(const BotsScreen(), [mac]));
    await _settle(t);
    // Demo bot has pending approval → inbox shows it.
    expect(find.text('Deny'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    await _settle(t);
  });
}
