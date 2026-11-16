import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/tree/node_ring.dart';

/// One roster row, as omp's Agent Hub draws it: status ring, dotted name, `↳ parent`,
/// type · model · thinking, the task in one line, and the numbers with the last activity.
class HubRow extends StatelessWidget {
  const HubRow({
    super.key,
    required this.node,
    required this.tree,
    required this.depth,
    required this.focused,
    required this.showParent,
    required this.fallbackAgent,
    required this.onTap,
  });

  final AgentNode node;
  final AgentTree tree;
  final int depth;
  final bool focused;

  /// Flat layout: name the parent on the row (the tree shows it by indentation).
  final bool showParent;
  final String fallbackAgent;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final n = node;
    final parent = n.isRoot ? null : tree.node(n.parent ?? '');
    final meta = hubMeta(n, fallbackAgent);
    final stats = hubStats(n);
    final last = hubLastActivity(n);
    return Padding(
      padding: EdgeInsets.only(left: depth * Space.lg, top: Space.sm),
      child: Material(
        color: focused ? c.accent.withValues(alpha: .10) : c.base,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(Space.radius),
          side: BorderSide(
            color: focused ? c.accent.withValues(alpha: .5) : c.borderSubtle,
          ),
        ),
        clipBehavior: Clip.antiAlias,
        child: InkWell(
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.all(Space.md),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                NodeRing(node: n, fallbackAgent: fallbackAgent, size: 30),
                const SizedBox(width: Space.md),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        n.isRoot ? 'Main' : n.id,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: Theme.of(context).textTheme.bodyLarge!
                            .copyWith(fontWeight: FontWeight.w700),
                      ),
                      if (showParent && parent != null)
                        Text(
                          '↳ ${parent.isRoot ? 'Main' : parent.id}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: Theme.of(context).textTheme.bodySmall!
                              .copyWith(color: c.muted),
                        ),
                      if (meta.isNotEmpty)
                        Text(
                          meta,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: Theme.of(context).textTheme.bodySmall!
                              .copyWith(color: c.muted),
                        ),
                      if (hubTask(n).isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            hubTask(n),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: Theme.of(context).textTheme.bodyMedium!
                                .copyWith(color: c.text),
                          ),
                        ),
                      if (stats.isNotEmpty || last.isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: Space.xs),
                          child: Text(
                            [
                              stats,
                              last,
                            ].where((s) => s.isNotEmpty).join(' · '),
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: Theme.of(context).textTheme.bodySmall!
                                .copyWith(color: c.muted),
                          ),
                        ),
                    ],
                  ),
                ),
                Icon(Icons.chevron_right, size: 18, color: c.faint),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// `TASK · claude-sonnet-5-5 · high`: the agent definition, model and thinking level.
String hubMeta(AgentNode n, String fallbackAgent) => [
  (n.agentType ?? (n.isRoot ? fallbackAgent : n.agent ?? '')).toUpperCase(),
  if (n.model != null && n.model!.isNotEmpty) n.model!.split('/').last,
  if (n.thinking != null && n.thinking!.isNotEmpty && n.thinking != 'off')
    n.thinking!,
].where((s) => s.isNotEmpty).join(' · ');

/// The task as one line: the node's assignment, else (root) the session title.
String hubTask(AgentNode n) {
  final t = (n.isRoot ? n.name : n.task) ?? '';
  return t.trim().replaceAll(RegExp(r'\s+'), ' ');
}

/// `$0.31 · 4m 2s · 3 req · 12 tools · 48.2k tok`.
String hubStats(AgentNode n) => [
  costText(n.cost),
  if (n.activeMs > 0) durationText(n.activeMs),
  if (n.requests > 0) '${n.requests} req',
  if (n.toolCount > 0) '${n.toolCount} tools',
  if (n.tokens > 0) '${tokensText(n.tokens)} tok',
].where((s) => s.isNotEmpty).join(' · ');

/// What it did last: the running tool now, else its newest call and how long ago.
String hubLastActivity(AgentNode n) {
  if (n.running && n.now != null && n.now!.isNotEmpty) return n.now!;
  if (n.recent.isEmpty) return '';
  final r = n.recent.last;
  final ago = agoText(r.at);
  return [
    if (r.intent.isNotEmpty) r.intent else r.tool,
    if (ago.isNotEmpty) ago,
  ].join(' · ');
}
