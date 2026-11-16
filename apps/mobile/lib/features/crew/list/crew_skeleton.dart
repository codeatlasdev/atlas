import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/skeleton.dart';

/// The Crew tab before the crew arrives: the title, the deck's card and rows shaped like [CrewmateRow]
/// (54 dp face, name and status, one line of what it is doing), so the crew lands where this was.
class CrewSkeleton extends StatelessWidget {
  const CrewSkeleton({super.key});

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    const names = [.38, .5, .32, .44];
    const lines = [.72, .55, .63, .48];
    return Skeleton(
      child: ListView(
        physics: const NeverScrollableScrollPhysics(),
        padding: const EdgeInsets.only(top: Space.md),
        children: [
          const Padding(
            padding: EdgeInsets.fromLTRB(
              Space.lg,
              Space.sm,
              Space.lg,
              Space.lg,
            ),
            child: Row(
              children: [
                SkeletonBlock(width: 96, height: 22, radius: 8),
                Spacer(),
                SkeletonBlock(height: 32, circle: true),
              ],
            ),
          ),
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: Space.lg),
            child: SkeletonCard(
              child: Row(
                children: [
                  SkeletonBlock(height: 44, circle: true),
                  SizedBox(width: Space.md),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        SkeletonBlock(width: 140, height: 14),
                        SizedBox(height: 8),
                        SkeletonBlock(width: 190, height: 11),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: Space.md),
          for (var i = 0; i < 4; i++)
            Padding(
              padding: const EdgeInsets.only(left: Space.lg),
              child: Row(
                children: [
                  const SkeletonBlock(height: 54, circle: true),
                  const SizedBox(width: Space.md),
                  Expanded(
                    child: Container(
                      padding: const EdgeInsets.fromLTRB(
                        0,
                        Space.md + 2,
                        Space.lg,
                        Space.md + 2,
                      ),
                      decoration: BoxDecoration(
                        border: i == 3
                            ? null
                            : Border(bottom: BorderSide(color: c.borderSubtle)),
                      ),
                      child: LayoutBuilder(
                        builder: (_, box) => Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Row(
                              children: [
                                SkeletonBlock(
                                  width: box.maxWidth * names[i],
                                  height: 14,
                                ),
                                const Spacer(),
                                const SkeletonBlock(width: 36, height: 10),
                              ],
                            ),
                            const SizedBox(height: 9),
                            SkeletonBlock(
                              width: box.maxWidth * lines[i],
                              height: 11,
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}
