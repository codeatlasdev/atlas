// AttentionCard is the one "this needs you" surface: a tone-tinted card with a tone border. The numbers
// (fill and border alpha, radius, padding) used to be copied into five files; these pin the recipes so
// the migration leaves every screen as it was.
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/attention_card.dart';

Widget _host(Widget child) => MaterialApp(
  theme: atlasTheme(),
  home: Scaffold(
    body: Align(alignment: Alignment.topLeft, child: child),
  ),
);

BoxDecoration _deco(WidgetTester t) {
  final box = find.descendant(
    of: find.byType(AttentionCard),
    matching: find.byType(DecoratedBox),
  );
  return t.widget<DecoratedBox>(box.first).decoration as BoxDecoration;
}

void main() {
  const c = AtlasColors.dark;

  // Recipes taken verbatim from the call sites before they were merged.
  group('the recipes the five screens used', () {
    testWidgets(
      'quiet = pause_card / needs_you_card / action_card / ask_card: warning .08 fill, .35 border, 14 radius',
      (t) async {
        await t.pumpWidget(_host(const AttentionCard(child: Text('x'))));
        final d = _deco(t);
        expect(d.color, c.warning.withValues(alpha: .08));
        expect(
          (d.border! as Border).top.color,
          c.warning.withValues(alpha: .35),
        );
        expect(d.borderRadius, BorderRadius.circular(14));
        expect(t.getSize(find.byType(AttentionCard)), isNotNull);
        final pad = t
            .widget<Container>(
              find
                  .descendant(
                    of: find.byType(AttentionCard),
                    matching: find.byType(Container),
                  )
                  .first,
            )
            .padding;
        expect(pad, const EdgeInsets.all(14));
      },
    );

    testWidgets(
      'loud = approval_banner: warning .12 fill, .6 border, Space.radius',
      (t) async {
        await t.pumpWidget(
          _host(
            const AttentionCard(
              strength: AttentionStrength.loud,
              radius: BorderRadius.all(Radius.circular(Space.radius)),
              child: Text('x'),
            ),
          ),
        );
        final d = _deco(t);
        expect(d.color, c.warning.withValues(alpha: .12));
        expect(
          (d.border! as Border).top.color,
          c.warning.withValues(alpha: .6),
        );
        expect(d.borderRadius, BorderRadius.circular(Space.radius));
      },
    );

    testWidgets('firm = approval_bubble: warning .10 fill, .40 border', (
      t,
    ) async {
      await t.pumpWidget(
        _host(
          const AttentionCard(
            strength: AttentionStrength.firm,
            child: Text('x'),
          ),
        ),
      );
      final d = _deco(t);
      expect(d.color, c.warning.withValues(alpha: .10));
      expect((d.border! as Border).top.color, c.warning.withValues(alpha: .4));
    });
  });

  testWidgets(
    'tone picks the colour: danger = error, info = info, accent = accent',
    (t) async {
      for (final (tone, color) in [
        (AttentionTone.warning, c.warning),
        (AttentionTone.danger, c.error),
        (AttentionTone.info, c.info),
        (AttentionTone.accent, c.accent),
      ]) {
        await t.pumpWidget(
          _host(AttentionCard(tone: tone, child: const Text('x'))),
        );
        final d = _deco(t);
        expect(d.color, color.withValues(alpha: .08), reason: '$tone fill');
        expect(
          (d.border! as Border).top.color,
          color.withValues(alpha: .35),
          reason: '$tone border',
        );
      }
    },
  );

  testWidgets(
    'bubble: tail corner, 12 padding, never wider than 80% of the screen',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(400, 800);
      await t.pumpWidget(
        _host(
          AttentionCard(
            bubble: true,
            child: SizedBox(width: 1000, child: const Text('x')),
          ),
        ),
      );
      expect(
        _deco(t).borderRadius,
        const BorderRadius.only(
          topLeft: Radius.circular(4),
          topRight: Radius.circular(18),
          bottomLeft: Radius.circular(18),
          bottomRight: Radius.circular(18),
        ),
      );
      final box = find.descendant(
        of: find.byType(AttentionCard),
        matching: find.byType(Container),
      );
      expect(t.widget<Container>(box.first).padding, const EdgeInsets.all(12));
      expect(
        t.getSize(find.byType(AttentionCard)).width,
        lessThanOrEqualTo(320),
      );
      t.view.resetPhysicalSize();
      t.view.resetDevicePixelRatio();
    },
  );

  testWidgets('radius and padding overrides win over the defaults', (t) async {
    await t.pumpWidget(
      _host(
        const AttentionCard(
          bubble: true,
          radius: BorderRadius.all(Radius.circular(3)),
          padding: EdgeInsets.all(5),
          child: Text('x'),
        ),
      ),
    );
    expect(_deco(t).borderRadius, BorderRadius.circular(3));
    final box = find.descendant(
      of: find.byType(AttentionCard),
      matching: find.byType(Container),
    );
    expect(t.widget<Container>(box.first).padding, const EdgeInsets.all(5));
  });

  testWidgets('title, body and actions render in order; actions are optional', (
    t,
  ) async {
    await t.pumpWidget(
      _host(
        AttentionCard(
          icon: Icons.pause_circle_outline,
          title: 'Crew paused',
          body: 'Nothing runs until you resume.',
          actions: [TextButton(onPressed: () {}, child: const Text('Resume'))],
        ),
      ),
    );
    final title = t.getTopLeft(find.text('Crew paused'));
    final body = t.getTopLeft(find.text('Nothing runs until you resume.'));
    expect(body.dy, greaterThan(title.dy));
    expect(find.text('Resume'), findsOneWidget);
    expect(
      t.widget<Icon>(find.byIcon(Icons.pause_circle_outline)).color,
      c.warning,
    );

    await t.pumpWidget(_host(const AttentionCard(title: 'Only a title')));
    expect(find.text('Only a title'), findsOneWidget);
    expect(find.byType(TextButton), findsNothing);
  });

  testWidgets(
    'the card is one semantics node group labelled by its title (live region so a new ask is announced)',
    (t) async {
      final handle = t.ensureSemantics();
      await t.pumpWidget(
        _host(const AttentionCard(title: 'Wants to run Bash', body: 'ls')),
      );
      final n = t.getSemantics(find.byType(AttentionCard));
      expect(n.label, contains('Wants to run Bash'));
      expect(n.flagsCollection.isLiveRegion, isTrue);
      handle.dispose();
    },
  );

  testWidgets('arbitrary child content is allowed when no title is given', (
    t,
  ) async {
    await t.pumpWidget(_host(const AttentionCard(child: Text('custom body'))));
    expect(find.text('custom body'), findsOneWidget);
  });
}
