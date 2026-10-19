import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm/xterm.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/terminal/terminal_feed.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';

Future<Terminal> _feed(WidgetTester t) async {
  final term = Terminal()..resize(40, 60);
  term.write('❯ ls\r\n${List.generate(20, (i) => 'line$i').join('\r\n')}\r\n');
  await t.pumpWidget(
    MaterialApp(
      theme: atlasTheme(Brightness.dark),
      home: Scaffold(
        body: TerminalFeed(terminal: term, theme: atlasTerminalTheme),
      ),
    ),
  );
  await t.pump();
  return term;
}

void main() {
  // Guards copy: it was on long-press, which SelectableText takes for selection,
  // so copying a block never happened.
  testWidgets('copy puts the whole block on the clipboard, folded lines too', (
    t,
  ) async {
    String? clip;
    t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (c) async {
        if (c.method == 'Clipboard.setData') {
          clip = (c.arguments as Map)['text'] as String;
        }
        return null;
      },
    );
    await _feed(t);
    await t.tap(find.byTooltip('Copy block'));
    await t.pump();
    final lines = clip!.split('\n');
    expect(lines.first, '❯ ls');
    expect(lines.sublist(1), [for (var i = 0; i < 20; i++) 'line$i']);
  });

  testWidgets('the latest block folds to its tail and expands on tap', (
    t,
  ) async {
    await _feed(t);
    String shown() => t
        .widget<SelectableText>(find.byType(SelectableText))
        .textSpan!
        .toPlainText();
    expect(shown(), startsWith('line6'));
    expect(shown(), endsWith('line19'));
    await t.tap(find.text('Show 6 more lines'));
    await t.pump();
    expect(shown(), startsWith('line0'));
    expect(find.text('Show 6 more lines'), findsNothing);
  });
}
