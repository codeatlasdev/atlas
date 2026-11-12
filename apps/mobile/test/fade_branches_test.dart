// Guards the 2026-11-03 bug: going Crew -> Computers moved the tab bar but the
// Crew page stayed on screen, because the branch being left had its tickers
// muted before its own fade-out could run, freezing it at full opacity on top.
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/ui/fade_branches.dart';

/// The opacity actually rendered for the page showing [label].
double _shown(WidgetTester tester, String label) => tester
    .renderObject<RenderAnimatedOpacity>(
      find
          .ancestor(of: find.text(label), matching: find.byType(FadeTransition))
          .first,
    )
    .opacity
    .value;

void main() {
  Widget app(int index, {List<Widget>? pages}) => MaterialApp(
    home: FadeBranches(
      index: index,
      children: pages ?? const [Text('computers'), Text('crew')],
    ),
  );

  testWidgets('going back to the first tab fades the second page out', (
    tester,
  ) async {
    await tester.pumpWidget(app(0));
    await tester.pumpWidget(app(1));
    await tester.pumpAndSettle();
    expect(_shown(tester, 'crew'), 1);
    expect(_shown(tester, 'computers'), 0);

    await tester.pumpWidget(app(0));
    await tester.pumpAndSettle();
    expect(_shown(tester, 'crew'), 0);
    expect(_shown(tester, 'computers'), 1);
  });

  testWidgets('the hidden page on top does not swallow taps', (tester) async {
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
}
