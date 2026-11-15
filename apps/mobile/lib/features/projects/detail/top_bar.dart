import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// Back, and the logs shortcut; the title only while there is no header to carry it.
class ProjectTopBar extends StatelessWidget {
  const ProjectTopBar({
    super.key,
    required this.onLogs,
    required this.title,
    this.reconnecting = false,
  });
  final VoidCallback? onLogs;
  final String? title;

  /// The connection to the Mac is being re-established.
  final bool reconnecting;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: Space.xs),
    child: Row(
      children: [
        const BackButton(),
        if (title != null)
          Expanded(
            child: Text(
              title!,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context).textTheme.titleMedium,
            ),
          )
        else
          const Spacer(),
        if (reconnecting)
          Flexible(
            child: Padding(
              padding: const EdgeInsets.only(right: Space.sm),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const SizedBox.square(
                    dimension: 12,
                    child: CircularProgressIndicator(strokeWidth: 1.6),
                  ),
                  const SizedBox(width: 6),
                  Flexible(
                    child: Text(
                      'Reconnecting…',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.bodySmall!
                          .copyWith(color: AtlasColors.of(context).muted),
                    ),
                  ),
                ],
              ),
            ),
          ),
        if (onLogs != null)
          TextButton.icon(
            onPressed: onLogs,
            icon: const Icon(Icons.subject_rounded, size: 18),
            label: const Text('Logs'),
          ),
      ],
    ),
  );
}
