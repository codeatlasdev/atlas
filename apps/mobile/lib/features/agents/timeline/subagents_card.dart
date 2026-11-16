import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/agent_history.dart';
import 'package:atlas_mobile/core/protocol/timeline_items.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';

/// A `task` call: the subagents it started, each with its live state and report preview.
class SubagentsCard extends StatelessWidget {
  const SubagentsCard({
    super.key,
    required this.call,
    required this.history,
    required this.onOpen,
  });

  final ToolItem call;
  final AgentHistory history;
  final void Function(String name) onOpen;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final n = call.agents.length;
    return Container(
      decoration: BoxDecoration(
        color: c.base,
        border: Border.all(color: c.borderSubtle),
        borderRadius: BorderRadius.circular(Space.radius),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
              Space.md,
              Space.sm + 2,
              Space.md,
              Space.sm,
            ),
            child: Row(
              children: [
                Icon(Icons.hub_outlined, size: 16, color: c.muted),
                const SizedBox(width: Space.sm),
                Text(
                  n == 1 ? '1 subagent' : '$n subagents',
                  style: Theme.of(context).textTheme.bodyMedium!
                      .copyWith(fontWeight: FontWeight.w600, color: c.text),
                ),
              ],
            ),
          ),
          for (final a in call.agents)
            _SubagentRow(
              spawned: a,
              info: history.subagent(a.name),
              // Without live info yet the call's own status says if it is still going.
              callRunning: call.status == 'running',
              onTap: () => onOpen(a.name),
            ),
        ],
      ),
    );
  }
}

class _SubagentRow extends StatelessWidget {
  const _SubagentRow({
    required this.spawned,
    required this.info,
    required this.callRunning,
    required this.onTap,
  });

  final SpawnedAgent spawned;
  final SubagentInfo? info;
  final bool callRunning;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final running = info?.running ?? callRunning;
    final report = info?.report?.trim().replaceAll(RegExp(r'\s+'), ' ');
    return InkWell(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(
          horizontal: Space.md,
          vertical: Space.sm + 2,
        ),
        decoration: BoxDecoration(
          border: Border(top: BorderSide(color: c.borderSubtle)),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 1),
              child: StatusDot(
                color: running ? c.accent : c.success,
                pulse: running,
                size: 7,
              ),
            ),
            const SizedBox(width: Space.sm),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Flexible(
                        child: Text(
                          spawned.name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: Theme.of(context).textTheme.bodyMedium!
                              .copyWith(fontWeight: FontWeight.w600),
                        ),
                      ),
                      if (spawned.agent != null) ...[
                        const SizedBox(width: Space.sm),
                        AtlasChip(spawned.agent!, tone: c.muted),
                      ],
                      const SizedBox(width: Space.sm),
                      Text(
                        running ? 'running' : 'done',
                        style: Theme.of(context).textTheme.bodySmall!.copyWith(
                          fontWeight: FontWeight.w600,
                          color: running ? c.accent : c.success,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 2),
                  Text(
                    spawned.task,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: Theme.of(context).textTheme.bodySmall!
                        .copyWith(color: c.muted),
                  ),
                  if (report != null && report.isNotEmpty) ...[
                    const SizedBox(height: Space.xs),
                    Text(
                      report,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.bodySmall!
                          .copyWith(color: c.text, height: 1.35),
                    ),
                  ],
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Icon(Icons.chevron_right, size: 18, color: c.faint),
            ),
          ],
        ),
      ),
    );
  }
}
