import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

/// The project's setup command and its state, shown while it has not finished OK.
class ProjectSetupRow extends StatelessWidget {
  const ProjectSetupRow({super.key, required this.snap});
  final ProjectSnapshot snap;

  @override
  Widget build(BuildContext context) {
    final ok = snap.setupState == 'ok';
    return AtlasCard(
      child: Row(
        children: [
          Icon(
            ok
                ? Icons.check
                : snap.setupState == 'running'
                ? Icons.hourglass_top
                : Icons.error_outline,
            size: 16,
            color: ok
                ? AtlasColors.of(context).success
                : AtlasColors.of(context).error,
          ),
          const SizedBox(width: 8),
          const Text('Setup', style: TextStyle(fontWeight: FontWeight.w600)),
          const SizedBox(width: 8),
          Expanded(
            flex: 3,
            child: Text(
              snap.setupCommand ?? '',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context).textTheme.bodySmall!.copyWith(
                color: AtlasColors.of(context).text,
                fontFamily: monoFamily,
                fontFamilyFallback: monoFallback,
              ),
            ),
          ),
          const SizedBox(width: 8),
          Flexible(
            child: Text(
              snap.setupState!,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: AtlasColors.of(context).text),
            ),
          ),
        ],
      ),
    );
  }
}
