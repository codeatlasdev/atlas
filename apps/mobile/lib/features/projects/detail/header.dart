import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/projects/project_avatar.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

String _dur(int s) {
  final h = s ~/ 3600, m = (s % 3600) ~/ 60;
  return h > 0
      ? '${h}h ${m}m'
      : m > 0
      ? '${m}m'
      : '${s}s';
}

/// The project head: avatar, name, folder, status pill with uptime and pid, and Start.
class ProjectHeader extends StatelessWidget {
  const ProjectHeader({super.key, required this.snap, required this.onUp});
  final ProjectSnapshot snap;
  final VoidCallback? onUp;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final failed = snap.services.where((s) => s.state == 'failed').length;
    final (label, color) = !snap.running
        ? ('Stopped', c.muted)
        : failed > 0
        ? ('$failed failed', c.error)
        : ('Running', c.success);
    final meta = [
      if (snap.running && snap.uptimeS != null) 'up ${_dur(snap.uptimeS!)}',
      if (snap.pid != null) 'pid ${snap.pid}',
    ].join(' · ');
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Hero(
                tag: 'project-${snap.id}',
                child: ProjectAvatar(name: snap.name, size: 56),
              ),
              const SizedBox(width: Space.md),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      snap.name,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.headlineSmall
                          ?.copyWith(
                            fontWeight: FontWeight.w700,
                            letterSpacing: -.5,
                          ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      snap.root.replaceFirst(RegExp(r'^/Users/[^/]+'), '~'),
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.bodySmall!.copyWith(
                        color: c.muted,
                        fontFamily: monoFamily,
                        fontFamilyFallback: monoFallback,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: Space.md),
          // Status and uptime on the left, Start on the right; on a narrow phone or large
          // text the Start button drops to its own line instead of pushing them off screen.
          Wrap(
            alignment: WrapAlignment.spaceBetween,
            crossAxisAlignment: WrapCrossAlignment.center,
            spacing: Space.sm,
            runSpacing: Space.sm,
            children: [
              LayoutBuilder(
                builder: (context, box) => ConstrainedBox(
                  constraints: BoxConstraints(maxWidth: box.maxWidth),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Container(
                        padding: const EdgeInsets.fromLTRB(2, 2, Space.md, 2),
                        decoration: BoxDecoration(
                          color: color.withValues(alpha: .12),
                          borderRadius: BorderRadius.circular(999),
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            StatusDot(
                              color: color,
                              pulse: snap.running && failed == 0,
                              size: 6,
                            ),
                            Text(
                              label,
                              style: Theme.of(context).textTheme.bodySmall!
                                  .copyWith(
                                    color: color,
                                    fontWeight: FontWeight.w600,
                                  ),
                            ),
                          ],
                        ),
                      ),
                      if (meta.isNotEmpty) ...[
                        const SizedBox(width: Space.sm),
                        Flexible(
                          child: Text(
                            meta,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: Theme.of(context).textTheme.bodySmall!
                                .copyWith(color: c.muted),
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
              if (!snap.running)
                FilledButton.icon(
                  onPressed: onUp,
                  icon: const Icon(Icons.play_arrow_rounded),
                  label: const Text('Start'),
                ),
            ],
          ),
        ],
      ),
    );
  }
}
