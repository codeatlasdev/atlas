import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/markdown/atlas_markdown.dart';
import 'package:atlas_mobile/core/ui/markdown/numeric_column.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> pump(WidgetTester tester, String md, {double width = 360}) {
  return tester.pumpWidget(
    MaterialApp(
      theme: atlasTheme(),
      home: Scaffold(
        body: SingleChildScrollView(
          child: SizedBox(width: width, child: AtlasMarkdown(md)),
        ),
      ),
    ),
  );
}

/// All rendered text, including rich-text spans, concatenated.
String plain(WidgetTester tester) => [
  for (final w in tester.widgetList<RichText>(find.byType(RichText)))
    w.text.toPlainText(),
].join('\n');

const table = '''
| Service | Memory |
|---|---:|
| api | 40 MB |
| web | 120 MB |
| db | 8.4 MB |
''';

void main() {
  testWidgets('GFM table renders cells, not pipe syntax', (tester) async {
    await pump(tester, table);
    expect(find.text('Service'), findsOneWidget);
    expect(find.text('api'), findsOneWidget);
    expect(find.text('120 MB'), findsOneWidget);
    expect(find.byType(Table), findsOneWidget);
    expect(plain(tester), isNot(contains('|---')));
    expect(plain(tester), isNot(contains('|')));
  });

  testWidgets('table cells render inline markdown', (tester) async {
    await pump(tester, '| a | b |\n|---|---|\n| **bold** | `code` |\n');
    expect(plain(tester), contains('bold'));
    expect(plain(tester), isNot(contains('**')));
    expect(plain(tester), isNot(contains('`')));
  });

  testWidgets('numeric column offers a chart; tapping shows it', (
    tester,
  ) async {
    await pump(tester, table);
    expect(find.byKey(const ValueKey('atlas-md-chart')), findsNothing);
    await tester.tap(find.byKey(const ValueKey('atlas-md-chart-toggle')));
    await tester.pump();
    final chart = find.byKey(const ValueKey('atlas-md-chart'));
    expect(chart, findsOneWidget);
    // Title is the numeric column's header; labels come from the first column;
    // values keep their unit.
    expect(
      find.descendant(of: chart, matching: find.text('Memory')),
      findsOneWidget,
    );
    expect(
      find.descendant(of: chart, matching: find.text('web')),
      findsOneWidget,
    );
    expect(
      find.descendant(of: chart, matching: find.text('120 MB')),
      findsOneWidget,
    );
    expect(
      find.descendant(of: chart, matching: find.text('8.4 MB')),
      findsOneWidget,
    );
    // Toggling again hides it.
    await tester.tap(find.byKey(const ValueKey('atlas-md-chart-toggle')));
    await tester.pump();
    expect(chart, findsNothing);
  });

  testWidgets('table without a numeric column has no chart toggle', (
    tester,
  ) async {
    await pump(tester, '| a | b |\n|---|---|\n| x | y |\n| z | 3 |\n');
    expect(find.byKey(const ValueKey('atlas-md-chart-toggle')), findsNothing);
  });

  test('numeric column detection: units, separators, percentages, mixed', () {
    NumericColumn? find(List<List<String>> rows) => NumericColumn.find(rows);
    expect(
      find([
        ['a', '40'],
        ['b', '8.4'],
      ])!.values,
      [40, 8.4],
    );
    expect(
      find([
        ['a', '1,234'],
        ['b', '12'],
      ])!.values,
      [1234, 12],
    );
    final pct = find([
      ['a', '12%'],
      ['b', '50%'],
    ])!;
    expect(pct.values, [12.0, 50.0]);
    expect(pct.unit, '%');
    final gb = find([
      ['a', '8.4 GB'],
      ['b', '16 GB'],
    ])!;
    expect(gb.values, [8.4, 16.0]);
    expect(gb.unit, 'GB');
    // Mixed units, text cells, or a lone label column: no chart.
    expect(
      find([
        ['a', '8 GB'],
        ['b', '9 MB'],
      ]),
      isNull,
    );
    expect(
      find([
        ['a', '8'],
        ['b', 'n/a'],
      ]),
      isNull,
    );
    expect(
      find([
        ['1'],
        ['2'],
      ]),
      isNull,
    );
    // The first qualifying column wins, skipping text ones.
    expect(
      find([
        ['a', 'x', '3'],
        ['b', 'y', '4'],
      ])!.index,
      2,
    );
  });

  testWidgets('code block shows content and copy puts it on the clipboard', (
    tester,
  ) async {
    String? copied;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          copied = (call.arguments as Map)['text'] as String;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );
    await pump(tester, 'Run:\n\n```bash\necho "hi"\nls -la\n```\n');
    expect(plain(tester), contains('echo "hi"\nls -la'));
    expect(find.text('bash'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('atlas-md-copy')));
    await tester.pump();
    expect(copied, 'echo "hi"\nls -la');
    // The check icon resets itself after the delay: let the timer run out.
    await tester.pump(const Duration(seconds: 3));
  });

  testWidgets('unclosed fence (streaming) still renders as code', (
    tester,
  ) async {
    await pump(
      tester,
      'Working on it\n\n```rust\nfn main() {\n    println!("x");',
    );
    expect(plain(tester), contains('fn main() {'));
    expect(plain(tester), contains('println!("x");'));
    expect(find.text('rust'), findsOneWidget);
    expect(plain(tester), isNot(contains('```')));
  });

  testWidgets('mermaid renders as a labelled code block', (tester) async {
    await pump(tester, '```mermaid\ngraph TD\n  A-->B\n```');
    expect(find.text('mermaid'), findsOneWidget);
    expect(plain(tester), contains('A-->B'));
  });

  testWidgets('inline emphasis has no markers in the output', (tester) async {
    await pump(
      tester,
      'This is **bold**, *italic*, ~~gone~~ and a [link](https://example.com).',
    );
    final text = plain(tester);
    expect(text, contains('This is bold, italic, gone and a link.'));
    expect(text, isNot(contains('*')));
    expect(text, isNot(contains('~~')));
    expect(text, isNot(contains('](')));
  });

  testWidgets('inline code renders in a pill without backticks', (
    tester,
  ) async {
    await pump(tester, 'Run `atlas dev` now');
    expect(find.text('atlas dev'), findsOneWidget);
    expect(plain(tester), isNot(contains('`')));
  });

  testWidgets('headings, hr and blockquote drop their markers', (tester) async {
    await pump(tester, '# Title\n\n#### Small\n\n---\n\n> quoted\n');
    final text = plain(tester);
    expect(text, contains('Title'));
    expect(text, contains('Small'));
    expect(text, contains('quoted'));
    expect(text, isNot(contains('#')));
    expect(text, isNot(contains('>')));
    expect(find.byType(Divider), findsWidgets);
  });

  testWidgets('bullet, ordered, task and nested lists', (tester) async {
    await pump(
      tester,
      '- one\n  - nested **b**\n- two\n\n3. third\n4. fourth\n\n- [x] done\n- [ ] todo\n',
    );
    final text = plain(tester);
    expect(text, contains('one'));
    expect(text, contains('nested b'));
    expect(text, contains('3.'));
    expect(text, contains('4.'));
    expect(text, contains('done'));
    expect(text, isNot(contains('[x]')));
    expect(text, isNot(contains('[ ]')));
    expect(find.byIcon(Icons.check_box), findsOneWidget);
    expect(find.byIcon(Icons.check_box_outline_blank), findsOneWidget);
  });

  testWidgets('streaming growth re-parses the new text', (tester) async {
    await pump(tester, 'Hello **wor');
    expect(plain(tester), contains('Hello'));
    await pump(tester, 'Hello **world**');
    expect(plain(tester), contains('Hello world'));
    expect(plain(tester), isNot(contains('*')));
  });

  testWidgets('long documents render without throwing', (tester) async {
    final doc = StringBuffer();
    for (var i = 0; i < 300; i++) {
      doc.writeln(
        '## Section $i\n\nSome **text** with `code` and a [link](https://x.y).\n',
      );
      doc.writeln('- item a\n- item b\n');
    }
    await pump(tester, doc.toString());
    expect(tester.takeException(), isNull);
    expect(plain(tester), contains('Section 0'));
  });

  testWidgets('wide table scrolls horizontally instead of overflowing', (
    tester,
  ) async {
    final cols = List.generate(8, (i) => 'column number $i').join(' | ');
    final sep = List.filled(8, '---').join('|');
    final row = List.generate(8, (i) => 'value number $i').join(' | ');
    await pump(tester, '| $cols |\n|$sep|\n| $row |\n', width: 300);
    expect(tester.takeException(), isNull);
    expect(find.text('column number 7'), findsOneWidget);
  });
}
