import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/projects/charts.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

/// A service tile: state, port or note, memory sparkline. Tap for [ServiceSheet].
class ServiceCard extends StatelessWidget {
  const ServiceCard({
    super.key,
    required this.svc,
    required this.mem,
    required this.color,
    required this.busy,
    required this.onOp,
    required this.onLogs,
  });

  final ServiceRow svc;
  final List<double> mem;
  final Color color;
  final bool busy;
  final void Function(String op) onOp;
  final VoidCallback onLogs;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final failed = svc.state == 'failed';
    return AtlasCard(
      padding: const EdgeInsets.fromLTRB(
        Space.md,
        Space.sm + 2,
        Space.md,
        Space.sm + 2,
      ),
      accent: failed ? c.error : null,
      onTap: () => ServiceSheet.show(context, this),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              StatusDot(
                color: c.serviceState(svc.state),
                pulse: svc.state == 'starting',
                size: 6,
              ),
              Expanded(
                child: Text(
                  svc.name,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.bodyLarge!
                      .copyWith(fontWeight: FontWeight.w600),
                ),
              ),
              if (busy)
                const SizedBox.square(
                  dimension: 14,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
            ],
          ),
          const SizedBox(height: 2),
          Text(
            svc.note ??
                (svc.port != null && svc.up
                    ? '${svc.state} · :${svc.port}'
                    : svc.state),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: Theme.of(context).textTheme.bodySmall!
                .copyWith(color: failed ? c.error : c.muted),
          ),
          const SizedBox(height: Space.sm),
          SizedBox(
            height: 22,
            child: svc.up
                ? Sparkline(values: mem, color: color)
                : Align(
                    alignment: Alignment.centerLeft,
                    child: Container(height: 1, color: c.borderSubtle),
                  ),
          ),
          const SizedBox(height: Space.xs),
          Text(
            svc.up ? '${svc.memMb.round()} MB · ${svc.cpu.round()}%' : '—',
            style: Theme.of(context).textTheme.labelSmall!.copyWith(
              color: c.muted,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
        ],
      ),
    );
  }
}

/// A service's details and controls, from its tile: state, port, last log line,
/// dependencies, and restart / stop / start / logs.
class ServiceSheet extends StatelessWidget {
  const ServiceSheet(this.card, {super.key});
  final ServiceCard card;

  static Future<void> show(BuildContext context, ServiceCard card) =>
      showModalBottomSheet<void>(
        context: context,
        builder: (_) => ServiceSheet(card),
      );

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final svc = card.svc;
    final failed = svc.state == 'failed';
    void run(String op) {
      Navigator.pop(context);
      card.onOp(op);
    }

    return SafeArea(
      child: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, Space.lg),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                StatusDot(color: c.serviceState(svc.state), size: 8),
                Flexible(
                  flex: 3,
                  child: Text(
                    svc.name,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: Theme.of(context).textTheme.titleLarge,
                  ),
                ),
                const SizedBox(width: Space.sm),
                Flexible(
                  flex: 2,
                  child: Text(
                    svc.note ?? svc.state,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(color: failed ? c.error : c.muted),
                  ),
                ),
                const Spacer(),
                if (svc.port != null)
                  AtlasChip(
                    ':${svc.port}',
                    tone: c.accent,
                    mono: true,
                    onTap: () {
                      Clipboard.setData(
                        ClipboardData(text: 'http://localhost:${svc.port}'),
                      );
                      ScaffoldMessenger.of(context).showSnackBar(
                        SnackBar(content: Text('localhost:${svc.port} copied')),
                      );
                    },
                  ),
              ],
            ),
            if (svc.up) ...[
              const SizedBox(height: Space.md),
              SizedBox(
                height: 40,
                child: Sparkline(values: card.mem, color: card.color),
              ),
              const SizedBox(height: Space.xs),
              Text(
                '${svc.memMb.round()} MB · ${svc.cpu.round()}% CPU'
                '${svc.restarts > 0 ? ' · ${svc.restarts} restarts' : ''}',
                style: Theme.of(context).textTheme.bodySmall!
                    .copyWith(color: c.muted),
              ),
            ],
            if (svc.lastLine.isNotEmpty) ...[
              const SizedBox(height: Space.md),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(Space.sm + 2),
                decoration: BoxDecoration(
                  color: c.ground,
                  borderRadius: BorderRadius.circular(Space.radiusSm),
                ),
                child: Text(
                  svc.lastLine,
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.bodySmall!.copyWith(
                    fontFamily: monoFamily,
                    fontFamilyFallback: monoFallback,
                    color: failed ? c.error : c.muted,
                  ),
                ),
              ),
            ],
            if (svc.dependsOn.isNotEmpty) ...[
              const SizedBox(height: Space.sm),
              Text(
                'Needs ${svc.dependsOn.join(', ')}',
                style: Theme.of(context).textTheme.bodySmall!
                    .copyWith(color: c.muted),
              ),
            ],
            const SizedBox(height: Space.lg),
            Wrap(
              spacing: Space.sm,
              runSpacing: Space.sm,
              children: [
                if (svc.up) ...[
                  FilledButton.icon(
                    onPressed: () => run('restart'),
                    icon: const Icon(Icons.refresh_rounded, size: 18),
                    label: const Text('Restart'),
                  ),
                  OutlinedButton.icon(
                    onPressed: () => run('stop'),
                    icon: const Icon(Icons.stop_rounded, size: 18),
                    label: const Text('Stop'),
                  ),
                ] else
                  FilledButton.icon(
                    onPressed: () => run('start'),
                    icon: const Icon(Icons.play_arrow_rounded, size: 18),
                    label: const Text('Start'),
                  ),
                OutlinedButton.icon(
                  onPressed: () {
                    Navigator.pop(context);
                    card.onLogs();
                  },
                  icon: const Icon(Icons.subject_rounded, size: 18),
                  label: const Text('Logs'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
