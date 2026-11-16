import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/markdown/code_block.dart';
import 'package:atlas_mobile/features/git/git_models.dart';

class DiffPanel extends StatelessWidget {
  const DiffPanel({super.key, required this.diff, required this.loaded});
  final FileDiff? diff;
  final bool loaded;

  @override
  Widget build(BuildContext context) {
    final d = diff;
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.sm),
      child: !loaded
          ? const Padding(
              padding: EdgeInsets.all(Space.md),
              child: Center(child: CircularProgressIndicator()),
            )
          : d == null || d.binary || d.hunks.isEmpty
          ? const Padding(
              padding: EdgeInsets.all(Space.md),
              child: Text('No text changes to show (binary or empty).'),
            )
          : AtlasCodeBlock(code: d.asPatch(), language: 'diff'),
    );
  }
}
