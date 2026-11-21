import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/projects/project_avatar.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';
import 'package:atlas_mobile/features/projects/detail/kpis.dart';

String _dur(int s) {
  final h = s ~/ 3600, m = (s % 3600) ~/ 60;
  return h > 0
      ? '${h}h ${m}m'
      : m > 0
      ? '${m}m'
      : '${s}s';
}

/// The project head: avatar, name and folder, then one status panel: the state with its action (Start when
/// stopped, Stop when running) and the numbers under it. State, action and numbers are one block, so the action
/// always sits on the state it changes and nothing floats beside a pill.
class ProjectHeader extends StatelessWidget {
  const ProjectHeader({
    super.key,
    required this.snap,
    required this.onUp,
    required this.onDown,
  });
  final ProjectSnapshot snap;

  /// Starts the whole stack; null while a start is in flight.
  final VoidCallback? onUp;

  /// Stops the whole stack; null while a stop is in flight.
  final VoidCallback? onDown;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
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
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.headlineSmall?.copyWith(
                        fontWeight: FontWeight.w700,
                        letterSpacing: -.5,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      snap.root.replaceFirst(RegExp(r'^/Users/[^/]+'), '~'),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.bodySmall!.copyWith(
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
          const SizedBox(height: Space.lg),
          AtlasCard(
            margin: EdgeInsets.zero,
            padding: EdgeInsets.zero,
            child: Column(
              children: [
                _StateLine(snap: snap, onUp: onUp, onDown: onDown),
                Divider(height: 1, thickness: 1, color: c.borderSubtle),
                Padding(
                  padding: const EdgeInsets.all(Space.md),
                  child: ProjectKpis(snap: snap),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// What the project is doing, in words, and the one thing to do about it. Stopped: "Stopped", how many
/// services it has, and Start. Running: "Running" (or how many failed), uptime and pid.
class _StateLine extends StatelessWidget {
  const _StateLine({
    required this.snap,
    required this.onUp,
    required this.onDown,
  });
  final ProjectSnapshot snap;
  final VoidCallback? onUp;
  final VoidCallback? onDown;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
    final failed = snap.services.where((s) => s.state == 'failed').length;
    final n = snap.services.length;
    final (label, color) = !snap.running
        ? ('Stopped', c.muted)
        : failed > 0
        ? ('$failed failed', c.error)
        : ('Running', c.success);
    final detail = !snap.running
        ? (n == 0
              ? 'No services'
              : '$n ${n == 1 ? 'service' : 'services'}, none running')
        : [
            if (snap.uptimeS != null) 'up ${_dur(snap.uptimeS!)}',
            if (snap.pid != null) 'pid ${snap.pid}',
          ].join(' · ');
    return Padding(
      padding: const EdgeInsets.all(Space.md),
      // The action drops under the state on a narrow phone or with large text, never squeezing it. Full width:
      // a Wrap is only as wide as its widest line, and spaceBetween needs the card's width to push Start right.
      child: SizedBox(
        width: double.infinity,
        child: Wrap(
          alignment: WrapAlignment.spaceBetween,
          crossAxisAlignment: WrapCrossAlignment.center,
          spacing: Space.md,
          runSpacing: Space.md,
          children: [
            Semantics(
              label: [label, if (detail.isNotEmpty) detail].join(', '),
              excludeSemantics: true,
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  StatusDot(
                    color: color,
                    pulse: snap.running && failed == 0,
                    size: 8,
                  ),
                  const SizedBox(width: Space.sm),
                  Flexible(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          label,
                          style: t.bodyLarge!.copyWith(
                            color: snap.running ? color : c.text,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                        if (detail.isNotEmpty)
                          Text(
                            detail,
                            style: t.bodySmall!.copyWith(color: c.muted),
                          ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
            if (!snap.running)
              FilledButton.icon(
                onPressed: onUp,
                icon: const Icon(Icons.play_arrow_rounded, size: 20),
                label: Text(onUp == null ? 'Starting…' : 'Start'),
              ),
            // Started from here or from the desktop, the stack can always be stopped from here
            // (each service also stops on its own, from its tile).
            if (snap.running)
              OutlinedButton.icon(
                key: const Key('project-stop'),
                onPressed: onDown,
                icon: const Icon(Icons.stop_rounded, size: 20),
                label: Text(
                  onDown == null
                      ? context.l10n.mobile_projects_stack_stopping
                      : context.l10n.mobile_projects_stack_stop,
                ),
              ),
          ],
        ),
      ),
    );
  }
}
