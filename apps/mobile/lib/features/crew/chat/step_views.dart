/// A task's steps in a crewmate bubble: grouped phases, each a short list of step phrases.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/crew_text.dart';
import 'package:atlas_mobile/features/crew/steps.dart';

// ── Step groups view ──────────────────────────────────────────────────────────

class StepGroupsView extends StatelessWidget {
  const StepGroupsView({
    super.key,
    required this.groups,
    required this.running,
    required this.startedAt,
    required this.expanded,
    required this.onToggle,
  });

  final List<StepGroup<CrewmateStep>> groups;
  final bool running;
  final int? startedAt;
  final bool expanded;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    if (groups.isEmpty) {
      return Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (running) ...[
            SizedBox.square(
              dimension: 12,
              child: CircularProgressIndicator(strokeWidth: 2, color: c.accent),
            ),
            const SizedBox(width: 8),
          ],
          Text(
            running
                ? (startedAt != null
                      ? 'Working · ${crewElapsed(startedAt!, DateTime.now())}'
                      : 'Working…')
                : 'Done',
            style: TextStyle(color: running ? c.accent : c.muted, fontSize: 16),
          ),
        ],
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (running) ...[
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox.square(
                dimension: 12,
                child: CircularProgressIndicator(
                  strokeWidth: 2,
                  color: c.accent,
                ),
              ),
              const SizedBox(width: 8),
              Text(
                startedAt != null
                    ? 'Working · ${crewElapsed(startedAt!, DateTime.now())}'
                    : 'Working…',
                style: TextStyle(color: c.accent, fontSize: 16),
              ),
            ],
          ),
          const SizedBox(height: 6),
        ],
        for (var gi = 0; gi < groups.length; gi++) ...[
          _StepGroupRow(
            group: groups[gi],
            isLast: gi == groups.length - 1,
            running: running,
            expanded: expanded,
            onToggle: onToggle,
          ),
          if (gi < groups.length - 1) const SizedBox(height: 4),
        ],
      ],
    );
  }
}

// ── One step group row ────────────────────────────────────────────────────────

class _StepGroupRow extends StatelessWidget {
  const _StepGroupRow({
    required this.group,
    required this.isLast,
    required this.running,
    required this.expanded,
    required this.onToggle,
  });

  final StepGroup<CrewmateStep> group;
  final bool isLast;
  final bool running;
  final bool expanded;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final title = group.title ?? (running ? null : 'Steps');

    if (group.done) {
      // Finished group: '✓ Title · N steps', tap to expand.
      return GestureDetector(
        onTap: onToggle,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.check_circle_outline, size: 13, color: c.success),
                const SizedBox(width: 4),
                Flexible(
                  child: Text(
                    title != null
                        ? '$title · ${group.steps.length} step${group.steps.length == 1 ? '' : 's'}'
                        : '${group.steps.length} step${group.steps.length == 1 ? '' : 's'}',
                    style: TextStyle(fontSize: 12, color: c.muted),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                const SizedBox(width: 2),
                Icon(
                  expanded
                      ? Icons.keyboard_arrow_up
                      : Icons.keyboard_arrow_down,
                  size: 14,
                  color: c.faint,
                ),
              ],
            ),
            if (expanded) ...[
              const SizedBox(height: 4),
              _StepList(steps: group.steps),
            ],
          ],
        ),
      );
    }

    // Current (open) group: title + last 6 steps, '+N earlier' if more.
    const kMax = 6;
    final shown = group.steps.length > kMax
        ? group.steps.sublist(group.steps.length - kMax)
        : group.steps;
    final earlier = group.steps.length - shown.length;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (title != null) ...[
          Text(
            title,
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: c.muted,
            ),
          ),
          const SizedBox(height: 4),
        ],
        if (earlier > 0) ...[
          Text(
            '+$earlier earlier',
            style: TextStyle(
              fontSize: 11,
              color: c.faint,
              fontStyle: FontStyle.italic,
            ),
          ),
          const SizedBox(height: 2),
        ],
        _StepList(steps: shown),
      ],
    );
  }
}

// ── Step list ─────────────────────────────────────────────────────────────────

class _StepList extends StatelessWidget {
  const _StepList({required this.steps});
  final List<CrewmateStep> steps;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final step in steps)
          Padding(
            padding: const EdgeInsets.only(bottom: 2),
            child: Text(
              '· ${stepPhrase(step.text)}',
              style: TextStyle(
                fontSize: 12,
                color: c.muted,
                fontFamily: 'Menlo',
                fontFamilyFallback: const [
                  'SF Mono',
                  'RobotoMono',
                  'monospace',
                ],
              ),
            ),
          ),
      ],
    );
  }
}
