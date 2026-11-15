// The controls every screen builds from the theme must be big enough to hit on their own, at
// the default text size and at 2x, on both platforms' rules (Android 48 dp, iOS 44 pt) and with a
// label for a screen reader. A control that only passes because a screen pads it is a bug in the
// theme, found one screen at a time.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/glass.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

Future<void> _meetsAll(WidgetTester t, Widget child, double scale) async {
  final handle = t.ensureSemantics();
  t.platformDispatcher.textScaleFactorTestValue = scale;
  addTearDown(t.platformDispatcher.clearTextScaleFactorTestValue);
  await t.pumpWidget(
    MaterialApp(
      theme: atlasTheme(),
      home: Scaffold(body: Center(child: child)),
    ),
  );
  await expectLater(t, meetsGuideline(androidTapTargetGuideline));
  await expectLater(t, meetsGuideline(iOSTapTargetGuideline));
  await expectLater(t, meetsGuideline(labeledTapTargetGuideline));
  handle.dispose();
}

void main() {
  // The guidelines look at the hit area, which Material pads on its own. A button that is
  // drawn short is still a smaller button to aim at, so the theme's own height is pinned too.
  testWidgets('themed buttons are drawn at least 48 dp high', (t) async {
    await t.pumpWidget(
      MaterialApp(
        theme: atlasTheme(),
        home: Scaffold(
          body: Column(
            children: [
              FilledButton(
                key: const Key('filled'),
                onPressed: () {},
                child: const Text('Save'),
              ),
              OutlinedButton(
                key: const Key('outlined'),
                onPressed: () {},
                child: const Text('Cancel'),
              ),
            ],
          ),
        ),
      ),
    );
    for (final k in ['filled', 'outlined']) {
      // The button's hit area is padded to 48 by Material whatever the style says; the
      // surface it paints (its inner Material) is what minimumSize sizes.
      final surface = find.descendant(
        of: find.byKey(Key(k)),
        matching: find.byType(Material),
      );
      expect(
        t.getSize(surface.first).height,
        greaterThanOrEqualTo(48),
        reason: k,
      );
    }
  });

  testWidgets('the round glass button is hit over 48 dp, drawn as before', (
    t,
  ) async {
    final h = t.ensureSemantics();
    await t.pumpWidget(
      MaterialApp(
        theme: atlasTheme(),
        home: Scaffold(
          body: Center(
            child: GlassRoundButton(
              icon: Icons.add,
              label: 'Add',
              onTap: () {},
            ),
          ),
        ),
      ),
    );
    expect(t.getSize(find.byType(GlassRoundButton)), const Size(48, 48));
    // 44 plus the hairline border: what was drawn before the hit area grew.
    expect(t.getSize(find.byType(GlassPill)), const Size(46, 46));
    await expectLater(t, meetsGuideline(androidTapTargetGuideline));
    await expectLater(t, meetsGuideline(iOSTapTargetGuideline));
    await expectLater(t, meetsGuideline(labeledTapTargetGuideline));
    h.dispose();
  });
  for (final scale in [1.0, 2.0]) {
    group('themed controls at x$scale text', () {
      testWidgets('FilledButton', (t) async {
        await _meetsAll(
          t,
          FilledButton(onPressed: () {}, child: const Text('Save')),
          scale,
        );
      });
      testWidgets('OutlinedButton', (t) async {
        await _meetsAll(
          t,
          OutlinedButton(onPressed: () {}, child: const Text('Cancel')),
          scale,
        );
      });
      testWidgets('TextButton', (t) async {
        await _meetsAll(
          t,
          TextButton(onPressed: () {}, child: const Text('Logs')),
          scale,
        );
      });
      testWidgets('IconButton', (t) async {
        await _meetsAll(
          t,
          IconButton(
            onPressed: () {},
            tooltip: 'Close',
            icon: const Icon(Icons.close),
          ),
          scale,
        );
      });
      testWidgets('ChoiceChip', (t) async {
        await _meetsAll(
          t,
          ChoiceChip(
            label: const Text('All'),
            selected: true,
            onSelected: (_) {},
          ),
          scale,
        );
      });
      testWidgets('SegmentedButton', (t) async {
        await _meetsAll(
          t,
          SegmentedButton<bool>(
            segments: const [
              ButtonSegment(value: true, label: Text('Memory')),
              ButtonSegment(value: false, label: Text('CPU')),
            ],
            selected: const {true},
            onSelectionChanged: (_) {},
          ),
          scale,
        );
      });
      testWidgets('crew pill buttons', (t) async {
        await _meetsAll(
          t,
          Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              WarnPillButton(label: 'Allow', onPressed: () {}),
              OutlinePillButton(label: 'Deny', danger: true, onPressed: () {}),
            ],
          ),
          scale,
        );
      });
    });
  }
}
