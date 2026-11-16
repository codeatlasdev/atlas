import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/skeleton.dart';

/// The home's project list before it arrives: rows shaped like [ProjectTile] (50 dp avatar, name and state,
/// the agent line under it, a hairline between rows), so the real list lands exactly where this was.
class HomeProjectsSkeleton extends StatelessWidget {
  const HomeProjectsSkeleton({super.key, this.rows = 5});
  final int rows;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    // Varied widths read as text, equal ones read as a pattern.
    const names = [.42, .56, .34, .5, .38, .46];
    const lines = [.7, .52, .64, .44, .58, .5];
    return Skeleton(
      child: Column(
        children: [
          for (var i = 0; i < rows; i++)
            Padding(
              padding: const EdgeInsets.only(left: Space.lg),
              child: Row(
                children: [
                  const SkeletonBlock(height: 50, circle: true),
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
                        border: i == rows - 1
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
                                  width: box.maxWidth * names[i % names.length],
                                  height: 14,
                                ),
                                const Spacer(),
                                const SkeletonBlock(width: 44, height: 11),
                              ],
                            ),
                            const SizedBox(height: 9),
                            SkeletonBlock(
                              width: box.maxWidth * lines[i % lines.length],
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
