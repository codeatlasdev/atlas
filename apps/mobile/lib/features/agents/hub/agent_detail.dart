import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/hub/context_bar.dart';
import 'package:atlas_mobile/features/agents/hub/hub_row.dart';
import 'package:atlas_mobile/features/agents/tree/node_ring.dart';

/// The agent's detail as a sheet: Task, Current, Usage (with the context bar), Lineage
/// (tap to jump), Recent activity, and the ways to talk to it.
///
/// [tree] is read live; [id] picks the node. [onOpen] opens its conversation, [onJump]
/// shows another node's detail (lineage), [onMessage] (when given) starts a message
/// to/about it.
Future<void> showAgentDetail(
  BuildContext context, {
  required ValueNotifier<AgentTree> tree,
  required String id,
  required String fallbackAgent,
  required void Function(String id) onOpen,
  required void Function(String id) onJump,
  void Function(AgentNode node)? onMessage,
}) => showModalBottomSheet<void>(
  context: context,
  showDragHandle: true,
  isScrollControlled: true,
  backgroundColor: AtlasColors.of(context).overlay,
  builder: (ctx) => ValueListenableBuilder<AgentTree>(
    valueListenable: tree,
    builder: (ctx, t, _) {
      final node = t.node(id);
      if (node == null) {
        return const SafeArea(
          child: Padding(
            padding: EdgeInsets.all(Space.xl),
            child: Text('This agent is gone'),
          ),
        );
      }
      return AgentDetail(
        node: node,
        tree: t,
        fallbackAgent: fallbackAgent,
        onOpen: (x) {
          Navigator.pop(ctx);
          onOpen(x);
        },
        onJump: (x) {
          Navigator.pop(ctx);
          onJump(x);
        },
        onMessage: onMessage == null
            ? null
            : (m) {
                Navigator.pop(ctx);
                onMessage(m);
              },
      );
    },
  ),
);

/// The body of [showAgentDetail].
class AgentDetail extends StatelessWidget {
  const AgentDetail({
    super.key,
    required this.node,
    required this.tree,
    required this.fallbackAgent,
    required this.onOpen,
    required this.onJump,
    this.onMessage,
  });

  final AgentNode node;
  final AgentTree tree;
  final String fallbackAgent;
  final void Function(String id) onOpen;
  final void Function(String id) onJump;
  final void Function(AgentNode node)? onMessage;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final n = node;
    final theme = Theme.of(context).textTheme;
    final parent = n.isRoot ? null : tree.node(n.parent ?? '');
    final kids = tree.children(n.id);
    final task = hubTask(n);
    final share = n.contextShare;

    Widget section(String title, Widget child) => Padding(
      padding: const EdgeInsets.only(top: Space.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title, style: theme.titleSmall),
          const SizedBox(height: Space.xs),
          child,
        ],
      ),
    );

    // The content scrolls; the ways to act on the agent stay pinned under it.
    return SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxHeight: MediaQuery.sizeOf(context).height * .88,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Flexible(
              child: SingleChildScrollView(
                padding: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, 0),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Row(
                      children: [
                        NodeRing(node: n, fallbackAgent: fallbackAgent),
                        const SizedBox(width: Space.md),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                n.isRoot ? 'Main' : n.id,
                                style: theme.titleMedium,
                              ),
                              Text(
                                [
                                  nodeStatusLabel(n),
                                  hubMeta(n, fallbackAgent),
                                ].where((s) => s.isNotEmpty).join(' · '),
                                style: Theme.of(context).textTheme.bodySmall!
                                    .copyWith(color: c.muted),
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                    if (task.isNotEmpty)
                      section(
                        'Task',
                        SelectableText(
                          n.isRoot ? task : (n.task ?? ''),
                          style: TextStyle(color: c.text, height: 1.4),
                        ),
                      ),
                    if (n.running && (n.now ?? '').isNotEmpty)
                      section(
                        'Current',
                        Row(
                          children: [
                            StatusDot(color: c.accent, pulse: true, size: 7),
                            const SizedBox(width: Space.sm),
                            Expanded(
                              child: Text(
                                n.now!,
                                style: TextStyle(color: c.text, height: 1.4),
                              ),
                            ),
                          ],
                        ),
                      ),
                    section(
                      'Usage',
                      Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            [
                                  costText(n.cost),
                                  if (n.activeMs > 0)
                                    '${durationText(n.activeMs)} active',
                                  if (n.requests > 0) '${n.requests} req',
                                  if (n.toolCount > 0) '${n.toolCount} tools',
                                  if (n.tokens > 0)
                                    '${tokensText(n.tokens)} tok',
                                ]
                                .where((s) => s.isNotEmpty)
                                .join(' · ')
                                .ifEmpty('—'),
                            key: const ValueKey('detail-usage'),
                            style: TextStyle(color: c.muted, height: 1.4),
                          ),
                          if (share != null) ...[
                            const SizedBox(height: Space.sm),
                            ContextBar(
                              tokens: n.contextTokens!,
                              window: n.contextWindow!,
                            ),
                          ],
                        ],
                      ),
                    ),
                    if (parent != null || kids.isNotEmpty)
                      section(
                        'Lineage',
                        Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            if (parent != null)
                              _LineageLink(
                                key: const ValueKey('lineage-parent'),
                                prefix: 'spawned by',
                                label: parent.isRoot ? 'Main' : parent.id,
                                onTap: () => onJump(parent.id),
                              ),
                            if (kids.isNotEmpty)
                              Padding(
                                padding: const EdgeInsets.only(top: 2),
                                child: Text(
                                  '${kids.length} ${kids.length == 1 ? 'child' : 'children'}',
                                  style: Theme.of(context).textTheme.bodySmall!
                                      .copyWith(color: c.muted),
                                ),
                              ),
                            for (final k in kids)
                              _LineageLink(
                                key: ValueKey('lineage-child-${k.id}'),
                                prefix: '↳',
                                label: k.id,
                                onTap: () => onJump(k.id),
                              ),
                          ],
                        ),
                      ),
                    if (n.todos != null && n.todos!.total > 0)
                      section(
                        'Todos',
                        Text(
                          '${n.todos!.done}/${n.todos!.total} done'
                          '${n.todos!.current == null ? '' : ' · ${n.todos!.current!.content}'}',
                          style: TextStyle(color: c.muted, height: 1.4),
                        ),
                      ),
                    if (n.recent.isNotEmpty)
                      section(
                        'Recent activity',
                        Column(
                          key: const ValueKey('detail-recent'),
                          children: [
                            for (final r in n.recent.reversed) _RecentRow(r),
                          ],
                        ),
                      ),
                    if ((n.report ?? '').isNotEmpty)
                      section('Report', SelectableText(n.report!)),
                    const SizedBox(height: Space.md),
                  ],
                ),
              ),
            ),
            ConstrainedBox(
              constraints: BoxConstraints(
                maxHeight: MediaQuery.sizeOf(context).height * .3,
              ),
              child: SingleChildScrollView(
                child: Container(
                  width: double.infinity,
                  decoration: BoxDecoration(
                    border: Border(top: BorderSide(color: c.borderSubtle)),
                  ),
                  padding: const EdgeInsets.fromLTRB(
                    Space.lg,
                    Space.md,
                    Space.lg,
                    Space.md,
                  ),
                  child: Wrap(
                    spacing: Space.sm,
                    runSpacing: Space.sm,
                    children: [
                      FilledButton.tonal(
                        key: const ValueKey('detail-open'),
                        onPressed: () => onOpen(n.id),
                        child: const Text('Open conversation'),
                      ),
                      if (onMessage != null)
                        OutlinedButton(
                          key: const ValueKey('detail-message'),
                          onPressed: () => onMessage!(n),
                          child: Text(
                            n.isRoot
                                ? 'Message'
                                : 'Message Main about ${n.label}',
                          ),
                        ),
                      if (parent != null)
                        OutlinedButton(
                          key: const ValueKey('detail-parent'),
                          onPressed: () => onOpen(parent.id),
                          child: Text(
                            parent.isRoot
                                ? 'Back to Main'
                                : 'Back to ${parent.label}',
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

extension on String {
  String ifEmpty(String other) => isEmpty ? other : this;
}

class _LineageLink extends StatelessWidget {
  const _LineageLink({
    super.key,
    required this.prefix,
    required this.label,
    required this.onTap,
  });
  final String prefix;
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return InkWell(
      onTap: onTap,
      child: MinTapTarget(
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              prefix,
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: c.muted),
            ),
            const SizedBox(width: Space.sm),
            Flexible(
              child: Text(
                label,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodyMedium!
                    .copyWith(fontWeight: FontWeight.w600, color: c.accent),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _RecentRow extends StatelessWidget {
  const _RecentRow(this.call);
  final RecentCall call;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (icon, color) = switch (call.status) {
      'ok' => (Icons.check_rounded, c.success),
      'error' => (Icons.close_rounded, c.error),
      _ => (Icons.autorenew_rounded, c.accent),
    };
    final at = clockText(call.at);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 56,
            child: Text(
              at,
              style: Theme.of(context).textTheme.bodySmall!.copyWith(
                color: c.muted,
                fontFamily: monoFamily,
                fontFamilyFallback: monoFallback,
              ),
            ),
          ),
          Icon(icon, size: 14, color: color),
          const SizedBox(width: Space.sm),
          Expanded(
            child: Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text: call.tool,
                    style: TextStyle(
                      fontWeight: FontWeight.w600,
                      color: c.text,
                    ),
                  ),
                  if (call.intent.isNotEmpty && call.intent != call.tool)
                    TextSpan(
                      text: '  ${call.intent}',
                      style: TextStyle(color: c.muted),
                    ),
                ],
              ),
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: AtlasColors.of(context).text, height: 1.3),
            ),
          ),
        ],
      ),
    );
  }
}
