// Guards the 2026-11-03 bug: going Crew -> Computers moved the tab bar but the Crew page stayed on screen. The
// frame-by-frame, real-shell version of "one page at a time" is test/tab_switch_test.dart; these are the parts
// of FadeBranches that need no app around it.
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/fade_branches.dart';

void main() {
  Widget app(int index, {List<Widget>? pages}) => MaterialApp(
    theme: atlasTheme(),
    home: FadeBranches(
      index: index,
      children: pages ?? const [Text('computers'), Text('crew')],
    ),
  );

  testWidgets('going back to the first tab leaves only that page painted', (
    tester,
  ) async {
    await tester.pumpWidget(app(0));
    await tester.pumpWidget(app(1));
    await tester.pumpAndSettle();
    expect(find.text('crew'), findsOneWidget);
    expect(
      find.text('computers'),
      findsNothing,
    ); // offstage pages are not found

    await tester.pumpWidget(app(0));
    await tester.pumpAndSettle();
    expect(find.text('computers'), findsOneWidget);
    expect(find.text('crew'), findsNothing);
  });

  testWidgets('the hidden page does not swallow taps', (tester) async {
    var computers = 0, crew = 0;
    final pages = [
      GestureDetector(onTap: () => computers++, child: const Text('computers')),
      GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: () => crew++,
        child: const SizedBox.expand(),
      ),
    ];
    await tester.pumpWidget(app(1, pages: pages));
    await tester.pumpWidget(app(0, pages: pages));
    await tester.pumpAndSettle();
    await tester.tap(find.text('computers'));
    expect((computers, crew), (1, 0));
  });

  testWidgets('a page keeps its state while another tab shows', (tester) async {
    final pages = [const _Counter(), const Text('crew')];
    await tester.pumpWidget(app(0, pages: pages));
    await tester.tap(find.byType(_Counter));
    await tester.pump();
    expect(find.text('1'), findsOneWidget);

    await tester.pumpWidget(app(1, pages: pages));
    await tester.pumpWidget(app(0, pages: pages));
    await tester.pumpAndSettle();
    expect(find.text('1'), findsOneWidget);
  });
}

class _Counter extends StatefulWidget {
  const _Counter();
  @override
  State<_Counter> createState() => _CounterState();
}

class _CounterState extends State<_Counter> {
  int n = 0;
  @override
  Widget build(BuildContext context) =>
      GestureDetector(onTap: () => setState(() => n++), child: Text('$n'));
}
