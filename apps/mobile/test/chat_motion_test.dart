// The crew chat felt static (2026-11-12): messages now arrive. Guards that an arrival
// plays once, from its sender's side, and settles exactly in place.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/features/crew/chat_motion.dart';

Widget _host(Widget child) => MaterialApp(
  home: Scaffold(
    body: Align(alignment: Alignment.topLeft, child: child),
  ),
);

double _opacity(WidgetTester t) =>
    t.widget<Opacity>(find.byType(Opacity)).opacity;

void main() {
  testWidgets('a new message slides in from its side and settles in place', (
    t,
  ) async {
    await t.pumpWidget(
      _host(const ChatAppear(fromRight: false, child: Text('hi'))),
    );
    await t.pump(const Duration(milliseconds: 60));
    final early = t.getTopLeft(find.text('hi'));
    expect(_opacity(t), lessThan(1));
    expect(early.dx, lessThan(0), reason: 'left sender: comes from the left');
    await t.pumpAndSettle();
    expect(_opacity(t), 1);
    expect(t.getTopLeft(find.text('hi')), Offset.zero);
  });

  testWidgets('a message already seen is simply there', (t) async {
    await t.pumpWidget(
      _host(const ChatAppear(fromRight: true, play: false, child: Text('old'))),
    );
    expect(_opacity(t), 1);
    expect(t.getTopLeft(find.text('old')), Offset.zero);
  });
}
