// AtlasChip is the one status/filter chip: a tinted capsule with an optional dot or icon, a selected
// state and, when it is tappable, a hit area of at least 48 dp however small the label is.
// Regression: the crew audit's filter chip was 36 dp tall (below the 48 dp touch target Material asks for).
import 'dart:ui' show Tristate;

import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';

Widget _host(Widget child) => MaterialApp(
  theme: atlasTheme(),
  home: Scaffold(body: Center(child: child)),
);

Size _hit(WidgetTester t, String label) {
  // The tappable region is the Semantics button wrapping the label, not the painted capsule.
  final node = find.ancestor(
    of: find.text(label),
    matching: find.byWidgetPredicate(
      (w) => w is Semantics && w.properties.button == true,
    ),
  );
  return t.getSize(node.first);
}

void main() {
  const c = AtlasColors.dark;

  testWidgets(
    'a tappable chip has a hit area of at least 48 dp, even for a one-letter label',
    (t) async {
      await t.pumpWidget(_host(AtlasChip('A', tone: c.accent, onTap: () {})));
      final s = _hit(t, 'A');
      expect(s.height, greaterThanOrEqualTo(48));
      expect(s.width, greaterThanOrEqualTo(48));
    },
  );

  testWidgets(
    'a selectable filter chip (the audit filters) is 48 dp tall too',
    (t) async {
      await t.pumpWidget(
        _host(
          AtlasChip(
            'All',
            tone: c.accent,
            selected: true,
            outlined: true,
            onTap: () {},
          ),
        ),
      );
      expect(_hit(t, 'All').height, greaterThanOrEqualTo(48));
    },
  );

  testWidgets(
    'tapping anywhere in the 48 dp area fires onTap, outside it does not',
    (t) async {
      var taps = 0;
      await t.pumpWidget(
        _host(AtlasChip('A', tone: c.accent, onTap: () => taps++)),
      );
      final centre = t.getCenter(find.text('A'));
      await t.tapAt(
        centre + const Offset(0, 20),
      ); // well below the painted capsule, inside 48 dp
      expect(taps, 1);
      await t.tapAt(
        centre + const Offset(0, 40),
      ); // beyond 24 dp from the centre
      expect(taps, 1);
    },
  );

  testWidgets(
    'a chip without onTap is not a button and takes no more room than its capsule',
    (t) async {
      await t.pumpWidget(_host(AtlasChip('3 failed', tone: c.error)));
      expect(find.bySemanticsLabel('3 failed'), findsOneWidget);
      expect(t.getSize(find.byType(AtlasChip)).height, lessThan(48));
      final handle = t.ensureSemantics();
      expect(
        t.getSemantics(find.text('3 failed')).flagsCollection.isButton,
        isFalse,
      );
      handle.dispose();
    },
  );

  testWidgets(
    'tappable chip exposes button + selected semantics with the label',
    (t) async {
      final handle = t.ensureSemantics();
      await t.pumpWidget(
        _host(
          AtlasChip('Failed', tone: c.accent, selected: true, onTap: () {}),
        ),
      );
      final n = t.getSemantics(find.text('Failed'));
      expect(n.label, 'Failed');
      expect(n.flagsCollection.isButton, isTrue);
      expect(n.flagsCollection.isSelected, Tristate.isTrue);
      await t.pumpWidget(
        _host(AtlasChip('Failed', tone: c.accent, onTap: () {})),
      );
      expect(
        t.getSemantics(find.text('Failed')).flagsCollection.isSelected,
        Tristate.isFalse,
      );
      handle.dispose();
    },
  );

  testWidgets(
    'a selected outlined chip is accent-tinted with an accent border; unselected is neutral',
    (t) async {
      BoxDecoration deco(WidgetTester t) {
        final box = find.descendant(
          of: find.byType(AtlasChip),
          matching: find.byType(DecoratedBox),
        );
        return t.widget<DecoratedBox>(box.first).decoration as BoxDecoration;
      }

      await t.pumpWidget(
        _host(
          AtlasChip(
            'x',
            tone: c.accent,
            selected: true,
            outlined: true,
            onTap: () {},
          ),
        ),
      );
      var d = deco(t);
      expect(d.color, c.accent.withValues(alpha: .16));
      expect((d.border! as Border).top.color, c.accent);

      await t.pumpWidget(
        _host(AtlasChip('x', tone: c.accent, outlined: true, onTap: () {})),
      );
      d = deco(t);
      expect(d.color, c.base);
      expect((d.border! as Border).top.color, c.borderSubtle);
    },
  );

  testWidgets(
    'a dot leading shows a StatusDot in the chip tone; pulse is passed through',
    (t) async {
      await t.pumpWidget(
        _host(
          AtlasChip(
            'Working',
            tone: c.success,
            leading: const ChipDot(pulse: true),
          ),
        ),
      );
      final dot = t.widget<StatusDot>(find.byType(StatusDot));
      expect(dot.color, c.success);
      expect(dot.pulse, isTrue);
      expect(dot.size, 6);
    },
  );

  testWidgets('an icon leading shows the icon in the chip tone', (t) async {
    await t.pumpWidget(
      _host(
        AtlasChip(
          'Paused',
          tone: c.warning,
          leading: const ChipIcon(Icons.pause),
        ),
      ),
    );
    final icon = t.widget<Icon>(find.byIcon(Icons.pause));
    expect(icon.color, c.warning);
  });

  testWidgets('a long label ellipsizes on one line instead of overflowing', (
    t,
  ) async {
    await t.pumpWidget(
      _host(
        SizedBox(
          width: 90,
          child: AtlasChip(
            'a very long status label indeed',
            tone: c.accent,
            leading: const ChipDot(),
          ),
        ),
      ),
    );
    expect(t.takeException(), isNull);
    final text = t.widget<Text>(find.textContaining('a very long'));
    expect(text.maxLines, 1);
    expect(text.overflow, TextOverflow.ellipsis);
  });

  testWidgets('mono uses the monospace family', (t) async {
    await t.pumpWidget(_host(AtlasChip(':3000', tone: c.accent, mono: true)));
    expect(t.widget<Text>(find.text(':3000')).style!.fontFamily, monoFamily);
  });
}
