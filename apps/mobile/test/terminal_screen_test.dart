import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm/xterm.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_transport.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

String _screen(WidgetTester t) {
  final term = t.widget<TerminalView>(find.byType(TerminalView)).terminal;
  return [
    for (var i = 0; i < term.buffer.lines.length; i++)
      term.buffer.lines[i].toString(),
  ].join('\n');
}

void main() {
  // Guards the blank agent terminal: hostd sends the replay before it answers
  // pty_attach, and the screen only started listening after the answer, so the
  // whole history was dropped. The demo transport keeps hostd's order.
  testWidgets('agent terminal shows the replay sent before the attach reply', (
    t,
  ) async {
    await t.pumpWidget(
      ProviderScope(
        overrides: [
          macStorageProvider.overrideWithValue(_Storage()),
          transportFactoryProvider.overrideWithValue(
            (_, _) async => demoTransport(),
          ),
        ],
        child: MaterialApp(
          theme: atlasTheme(Brightness.dark),
          home: TerminalScreen(
            device: demoDevice,
            pty: 'pty-demo-claude',
            title: 'claude',
          ),
        ),
      ),
    );
    for (var i = 0; i < 10; i++) {
      await t.pump(const Duration(milliseconds: 50));
    }
    expect(_screen(t), contains('~/Projetos/orbit-shop'));
    // The app bar names the agent, never the pty id.
    expect(find.text('claude'), findsOneWidget);
    expect(find.text('pty-demo-claude'), findsNothing);
    await t.pumpWidget(const SizedBox());
    await t.pump(const Duration(seconds: 3));
  });
}
