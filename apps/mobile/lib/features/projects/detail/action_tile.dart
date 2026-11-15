import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

/// An atlas.yaml action: what it is, whether it asks first, one tap to run.
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
    return AtlasCard(
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
                if (action.confirm != null)
                  Text(
                    'Asks before running',
                    style: TextStyle(color: c.muted, fontSize: 12),
                  ),
              ],
            ),
          ),
          const SizedBox(width: Space.sm),
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 96),
            child: Text(
              action.key,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                color: c.muted,
                fontFamily: monoFamily,
                fontFamilyFallback: monoFallback,
                fontSize: 12,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
