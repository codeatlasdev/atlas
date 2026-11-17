// Golden: every AtlasChip variant and every AttentionCard tone / strength / shape in one frame, so a change
// to either shows up as a picture. No pulse (a running animation never settles in a golden).
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/attention_card.dart';
import 'package:atlas_mobile/core/ui/components.dart';

import 'support/golden_frame.dart';

class _Fixture extends StatelessWidget {
  const _Fixture();

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return ListView(
      padding: const EdgeInsets.all(Space.lg),
      children: [
        Wrap(
          spacing: Space.sm,
          runSpacing: Space.sm,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            AtlasChip('3 failed', tone: c.error),
            AtlasChip('omp', tone: c.muted),
            AtlasChip(':3000', tone: c.accent, mono: true),
            AtlasChip('Working', tone: c.success, leading: const ChipDot()),
            AtlasChip('Needs you', tone: c.warning, leading: const ChipDot()),
            AtlasChip(
              'Paused',
              tone: c.warning,
              leading: const ChipIcon(Icons.pause),
            ),
            AtlasChip('Tap me', tone: c.accent, onTap: () {}),
          ],
        ),
        const SizedBox(height: Space.md),
        Wrap(
          spacing: Space.sm,
          children: [
            AtlasChip(
              'All',
              tone: c.accent,
              outlined: true,
              selected: true,
              onTap: () {},
            ),
            AtlasChip(
              'Approvals',
              tone: c.accent,
              outlined: true,
              onTap: () {},
            ),
            AtlasChip('Settings', tone: c.accent, outlined: true, onTap: () {}),
          ],
        ),
        const SizedBox(height: Space.lg),
        for (final tone in AttentionTone.values) ...[
          AttentionCard(
            tone: tone,
            icon: Icons.info_outline,
            title: 'Tone: ${tone.name}',
            body: 'The quiet strength, the card every crew screen uses.',
          ),
          const SizedBox(height: Space.sm),
        ],
        const AttentionCard(
          strength: AttentionStrength.loud,
          radius: BorderRadius.all(Radius.circular(Space.radius)),
          child: Text('Loud: the agent approval banner'),
        ),
        const SizedBox(height: Space.sm),
        const Align(
          alignment: Alignment.centerLeft,
          child: AttentionCard(
            bubble: true,
            strength: AttentionStrength.firm,
            child: Text('Firm bubble: Deny / Allow in a chat'),
          ),
        ),
        const SizedBox(height: Space.sm),
        const Align(
          alignment: Alignment.centerLeft,
          child: AttentionCard(
            bubble: true,
            child: Text('Quiet bubble: a question or action card'),
          ),
        ),
      ],
    );
  }
}

void main() {
  testWidgets('chip and attention card variants', (t) async {
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = const Size(393, 940);
    addTearDown(t.view.reset);
    await t.pumpWidget(goldenFrame(const _Fixture()));
    await t.pump();
    await expectLater(
      find.byType(MaterialApp),
      matchesGoldenFile('goldens/chip_card_variants.png'),
    );
  });
}
