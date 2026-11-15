import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

/// The KPI strip: running services, errors, restarts, memory and CPU.
class ProjectKpis extends StatelessWidget {
  const ProjectKpis({super.key, required this.snap});
  final ProjectSnapshot snap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final mem = snap.memMb >= 1024
        ? '${(snap.memMb / 1024).toStringAsFixed(1)} GB'
        : '${snap.memMb.round()} MB';
    final allUp = snap.upCount == snap.services.length;
    // (label, value, colour when it needs attention)
    final items = [
      (
        'running',
        '${snap.upCount}/${snap.services.length}',
        allUp ? null : c.error,
      ),
      ('errors · 5m', '${snap.errors5m}', snap.errors5m > 0 ? c.warning : null),
      ('restarts', '${snap.restarts}', snap.restarts > 0 ? c.warning : null),
      ('memory', mem, null),
      ('cpu', '${snap.cpu.round()}%', null),
    ];
    // One strip, scrollable on narrow phones, like the desktop's KPI row.
    return AtlasCard(
      padding: const EdgeInsets.symmetric(vertical: Space.md),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: Space.md),
        child: IntrinsicHeight(
          child: Row(
            children: [
              for (var i = 0; i < items.length; i++) ...[
                if (i > 0)
                  VerticalDivider(color: c.borderSubtle, width: Space.xl),
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    AnimatedSwitcher(
                      duration: Motion.normal,
                      transitionBuilder: (w, a) =>
                          FadeTransition(opacity: a, child: w),
                      child: Text(
                        items[i].$2,
                        key: ValueKey(items[i].$2),
                        style: Theme.of(context).textTheme.titleLarge!.copyWith(
                          fontWeight: FontWeight.w700,
                          color: items[i].$3 ?? c.text,
                          fontFeatures: const [FontFeature.tabularFigures()],
                        ),
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      items[i].$1,
                      style: Theme.of(context).textTheme.labelSmall!
                          .copyWith(color: c.muted),
                    ),
                  ],
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
