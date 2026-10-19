import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_transport.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// What the readable feed shows: every block's command and output text.
String _screen(WidgetTester t) => [
  for (final w in t.widgetList<SelectableText>(find.byType(SelectableText)))
    w.textSpan!.toPlainText(),
  for (final w in t.widgetList<RichText>(find.byType(RichText)))
    w.text.toPlainText(),
].join('\n');

/// A computer whose pty output splits "é" (C3 A9) across two pushes, the way a
/// real pty read can cut a multi-byte character at a chunk boundary.
class _SplitUtf8 implements Transport {
  final _lines = StreamController<String>.broadcast();
  bool _open = true;
  @override
  Stream<String> get incomingLines => _lines.stream;
  @override
  bool get isOpen => _open;
  @override
  Future<void> close() async => _open = false;
  @override
  Future<void> sendLine(String line) async {
    final m = jsonDecode(line) as Map;
    if (m['cmd'] == 'pty_attach') {
      for (final chunk in [
        [0x63, 0x61, 0x66, 0xC3],
        [0xA9, 0x21],
      ]) {
        _lines.add(
          jsonEncode({
            'ev': 'pty_output',
            'pty': 'p',
            'data': base64.encode(chunk),
          }),
        );
      }
    }
    scheduleMicrotask(
      () => _lines.add(jsonEncode({'id': m['id'], 'ok': true, 'data': {}})),
    );
  }
}

Future<void> _open(WidgetTester t, Transport Function() tr, String pty) async {
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage()),
        transportFactoryProvider.overrideWithValue((_, _) async => tr()),
      ],
      child: MaterialApp(
        theme: atlasTheme(Brightness.dark),
        home: TerminalScreen(device: demoDevice, pty: pty, title: 'claude'),
      ),
    ),
  );
  for (var i = 0; i < 10; i++) {
    await t.pump(const Duration(milliseconds: 50));
  }
}

void main() {
  // Guards the blank agent terminal: hostd sends the replay before it answers
  // pty_attach, and the screen only started listening after the answer, so the
  // whole history was dropped. The demo transport keeps hostd's order.
  testWidgets('agent terminal shows the replay sent before the attach reply', (
    t,
  ) async {
    await _open(t, demoTransport, 'pty-demo-claude');
    expect(_screen(t), contains('~/Projetos/orbit-shop'));
    // The app bar names the agent, never the pty id.
    expect(find.text('claude'), findsOneWidget);
    expect(find.text('pty-demo-claude'), findsNothing);
    await t.pumpWidget(const SizedBox());
    await t.pump(const Duration(seconds: 3));
  });

  // Guards garbled accents: each push was decoded alone (String.fromCharCodes),
  // so a character split between two pushes came out as two wrong ones.
  testWidgets('a UTF-8 character split across two pushes renders intact', (
    t,
  ) async {
    await _open(t, _SplitUtf8.new, 'p');
    expect(_screen(t), contains('café!'));
    await t.pumpWidget(const SizedBox());
    await t.pump(const Duration(seconds: 3));
  });
}
