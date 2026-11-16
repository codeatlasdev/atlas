import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

/// The project's numbers: running services, errors, restarts, memory and CPU. Laid out inside the header's
/// status panel (it draws no card of its own).
class ProjectKpis extends StatelessWidget {
  const ProjectKpis({super.key, required this.snap});
  final ProjectSnapshot snap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final mem = snap.memMb >= 1024
        ? '${(snap.memMb / 1024).toStringAsFixed(1)} GB'
        : '${snap.memMb.round()} MB';
    // A project you stopped is not an alarm: 0/6 is red only while it runs with services down, and memory and
    // CPU of nothing are a dash, not "0 MB".
    final down = snap.running && snap.upCount < snap.services.length;
    // (label, value, colour when it needs attention)
    final items = [
      (
        'running',
        '${snap.upCount}/${snap.services.length}',
        down ? c.error : null,
      ),
      ('errors · 5m', '${snap.errors5m}', snap.errors5m > 0 ? c.warning : null),
      ('restarts', '${snap.restarts}', snap.restarts > 0 ? c.warning : null),
      ('memory', snap.running ? mem : '—', null),
      ('cpu', snap.running ? '${snap.cpu.round()}%' : '—', null),
    ];
    // Every number in sight, never scrolled sideways: each at its own width, spread across the panel, wrapping
    // to a second line only when they do not fit (a narrow phone, large text).
    return SizedBox(
      width: double.infinity,
      child: Wrap(
        alignment: WrapAlignment.spaceBetween,
        spacing: Space.md,
        runSpacing: Space.md,
        children: [
          for (final (label, value, alert) in items)
            _Kpi(label: label, value: value, alert: alert),
        ],
      ),
    );
  }
}

class _Kpi extends StatelessWidget {
  const _Kpi({required this.label, required this.value, this.alert});
  final String label;
  final String value;

  /// The value's colour when it needs attention.
  final Color? alert;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
    return Semantics(
      label: '$label: $value',
      excludeSemantics: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          AnimatedSwitcher(
            duration: Motion.reduced(context) ? Duration.zero : Motion.normal,
            transitionBuilder: (w, a) => FadeTransition(opacity: a, child: w),
            child: Text(
              value,
              key: ValueKey(value),
              maxLines: 1,
              style: t.titleLarge!.copyWith(
                fontWeight: FontWeight.w700,
                color: alert ?? c.text,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
          ),
          const SizedBox(height: 2),
          Text(
            label,
            maxLines: 1,
            style: t.labelSmall!.copyWith(color: c.muted),
          ),
        ],
      ),
    );
  }
}
