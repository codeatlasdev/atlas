import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

/// An atlas.yaml action: what it is, whether it asks first, one tap to run. The desktop's keyboard key for it
/// (`action.key`) is not shown: it means nothing on a phone.
class ProjectActionTile extends StatelessWidget {
  const ProjectActionTile({
    super.key,
    required this.action,
    required this.onRun,
  });
  final ProjectAction action;
  final VoidCallback onRun;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final asks = action.confirm != null;
    return Semantics(
      button: true,
      label: action.label,
      hint: asks ? 'Asks before running' : null,
      excludeSemantics: true,
      onTap: onRun,
      child: AtlasCard(
        onTap: onRun,
        padding: const EdgeInsets.fromLTRB(
          Space.md,
          Space.sm + 2,
          Space.md,
          Space.sm + 2,
        ),
        child: Row(
          children: [
            Container(
              width: 32,
              height: 32,
              decoration: BoxDecoration(
                color: c.accent.withValues(alpha: .14),
                borderRadius: BorderRadius.circular(Space.radiusSm),
              ),
              child: Icon(Icons.play_arrow_rounded, color: c.accent, size: 20),
            ),
            const SizedBox(width: Space.md),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    action.label,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  if (asks)
                    Text(
                      'Asks before running',
                      style: Theme.of(context).textTheme.bodySmall!
                          .copyWith(color: c.muted),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
