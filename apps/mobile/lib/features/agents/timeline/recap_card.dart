import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/turn_recap.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';

/// The summary at the top of a turn: files touched, commands, subagents, time.
class RecapCard extends StatelessWidget {
  const RecapCard(this.recap, {super.key});
  final Recap recap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final r = recap;
    final files = r.files.length;
    final parts = <(IconData, String, Color)>[
      if (files > 0)
        (
          Icons.edit_note_rounded,
          '$files ${files == 1 ? 'file' : 'files'} changed',
          c.text,
        ),
      if (r.commands > 0)
        (
          Icons.terminal,
          '${r.commands} ${r.commands == 1 ? 'command' : 'commands'}',
          c.text,
        ),
      if (r.finished > 0)
        (
          Icons.hub_outlined,
          '${r.finished} ${r.finished == 1 ? 'subagent' : 'subagents'} done',
          c.success,
        ),
      if (r.failed > 0)
        (Icons.error_outline_rounded, '${r.failed} failed', c.error),
      if (r.duration != null && r.duration!.inSeconds > 0)
        (
          Icons.timer_outlined,
          durationText(r.duration!.inMilliseconds),
          c.muted,
        ),
    ];
    return Container(
      padding: const EdgeInsets.all(Space.md),
      decoration: BoxDecoration(
        color: c.accent.withValues(alpha: .07),
        border: Border.all(color: c.accent.withValues(alpha: .25)),
        borderRadius: BorderRadius.circular(Space.radius),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.auto_awesome, size: 14, color: c.accent),
              const SizedBox(width: Space.sm),
              Text(
                'Recap',
                style: Theme.of(context).textTheme.bodySmall!
                    .copyWith(fontWeight: FontWeight.w700, color: c.accent),
              ),
            ],
          ),
          const SizedBox(height: Space.sm),
          Wrap(
            spacing: Space.md,
            runSpacing: 4,
            children: [
              for (final (icon, label, color) in parts)
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(icon, size: 15, color: color),
                    const SizedBox(width: Space.xs),
                    Text(
                      label,
                      style: Theme.of(context).textTheme.bodyMedium!
                          .copyWith(color: color),
                    ),
                  ],
                ),
            ],
          ),
          if (files > 0) ...[
            const SizedBox(height: Space.sm),
            for (final f in r.files.take(4))
              Text(
                f,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodySmall!.copyWith(
                  fontFamily: monoFamily,
                  fontFamilyFallback: monoFallback,
                  color: c.muted,
                ),
              ),
            if (files > 4)
              Text(
                '+${files - 4} more',
                style: Theme.of(context).textTheme.bodySmall!
                    .copyWith(color: c.muted),
              ),
          ],
        ],
      ),
    );
  }
}
