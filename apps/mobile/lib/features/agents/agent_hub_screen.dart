import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/crew_widgets.dart';

/// How the hub lists agents: one list, or the spawn tree.
enum HubLayout { flat, byParent }

/// Which agents the hub lists.
enum HubFilter { all, running, done, failed }

bool _matches(HubFilter f, AgentNode n) => switch (f) {
  HubFilter.all => true,
  HubFilter.running => n.running || n.status == 'pending',
  HubFilter.done => n.done,
  HubFilter.failed => n.failed,
};

/// What the hub's header adds up over [nodes]: cost, time at work, requests, tool calls,
/// tokens.
class HubTotals {
  const HubTotals({
    this.cost = 0,
    this.activeMs = 0,
    this.requests = 0,
    this.tools = 0,
    this.tokens = 0,
  });
  final double cost;
  final int activeMs;
  final int requests;
  final int tools;
  final int tokens;

  factory HubTotals.of(Iterable<AgentNode> nodes) {
    var cost = 0.0;
    var active = 0, req = 0, tools = 0, tok = 0;
    for (final n in nodes) {
      cost += n.cost;
      active += n.activeMs;
      req += n.requests;
      tools += n.toolCount;
      tok += n.tokens;
    }
    return HubTotals(
      cost: cost,
      activeMs: active,
      requests: req,
      tools: tools,
      tokens: tok,
    );
  }

  /// `$0.31 · 4m 2s active · 12 req · 30 tools · 48.2k tok`, the unknown left out.
  String get line => [
    costText(cost),
    if (activeMs > 0) '${durationText(activeMs)} active',
    if (requests > 0) '$requests req',
    if (tools > 0) '$tools tools',
    if (tokens > 0) '${tokensText(tokens)} tok',
  ].where((s) => s.isNotEmpty).join(' · ');
}

/// The agent hub: every agent of the pane (the main one and all its subagents) as a
/// roster with running/parked counts, totals, a Flat / By parent switch and filters.
/// Tapping a row opens the agent's detail.
///
/// [tree] is read on every build of the owner, so the hub stays live while the chat
/// screen keeps polling.
class AgentHubScreen extends StatefulWidget {
  const AgentHubScreen({
    super.key,
    required this.tree,
    required this.focus,
    required this.fallbackAgent,
    required this.onFocus,
    this.onMessage,
    this.initialLayout = HubLayout.byParent,
  });

  final ValueNotifier<AgentTree> tree;

  /// The agent the chat shows now (marked in the list).
  final String focus;
  final String fallbackAgent;

  /// Opens that agent's conversation in the chat (`''` = the main agent).
  final void Function(String id) onFocus;

  /// Starts a message about / to that agent; null hides the button.
  final void Function(AgentNode node)? onMessage;
  final HubLayout initialLayout;

  @override
  State<AgentHubScreen> createState() => _AgentHubScreenState();
}

class _AgentHubScreenState extends State<AgentHubScreen> {
  late HubLayout _layout = widget.initialLayout;
  HubFilter _filter = HubFilter.all;

  /// Pops the hub, then runs [then]: the chat is underneath.
  void _leave(VoidCallback then) {
    Navigator.of(context).pop();
    then();
  }

  /// The detail of [id]; a lineage link re-opens it for the node it names.
  void _open(String id) => showAgentDetail(
    context,
    tree: widget.tree,
    id: id,
    fallbackAgent: widget.fallbackAgent,
    onOpen: (x) => _leave(() => widget.onFocus(x)),
    onJump: _open,
    onMessage: widget.onMessage == null
        ? null
        : (m) => _leave(() => widget.onMessage!(m)),
  );

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Agent hub')),
      body: ValueListenableBuilder<AgentTree>(
        valueListenable: widget.tree,
        builder: (context, t, _) {
          final all = t.flat;
          final running = t.nodes.where((n) => n.running).length;
          final parked = t.nodes.length - running;
          final totals = HubTotals.of(t.nodes);
          // The tree keeps each node's depth; the flat list drops it.
          final indent = _layout == HubLayout.byParent;
          final rows = <(AgentNode, int)>[
            for (final (n, d) in all)
              if (_matches(_filter, n)) (n, indent ? d : 0),
          ];
          return Column(
            children: [
              _HubHeader(
                running: running,
                parked: parked,
                totals: totals,
                layout: _layout,
                onLayout: (l) => setState(() => _layout = l),
                filter: _filter,
                onFilter: (f) => setState(() => _filter = f),
                tree: t,
              ),
              Expanded(
                child: rows.isEmpty
                    ? Center(
                        child: Text(
                          t.nodes.isEmpty
                              ? 'No agents yet'
                              : 'No agents match this filter',
                          style: TextStyle(color: c.faint),
                        ),
                      )
                    : ListView.builder(
                        padding: const EdgeInsets.fromLTRB(
                          Space.md,
                          Space.sm,
                          Space.md,
                          Space.xl,
                        ),
                        itemCount: rows.length,
                        itemBuilder: (_, i) {
                          final (n, depth) = rows[i];
                          return HubRow(
                            key: ValueKey('hub-${n.id}'),
                            node: n,
                            tree: t,
                            depth: depth,
                            focused: n.id == widget.focus,
                            showParent: _layout == HubLayout.flat,
                            fallbackAgent: widget.fallbackAgent,
                            onTap: () => _open(n.id),
                          );
                        },
                      ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class _HubHeader extends StatelessWidget {
  const _HubHeader({
    required this.running,
    required this.parked,
    required this.totals,
    required this.layout,
    required this.onLayout,
    required this.filter,
    required this.onFilter,
    required this.tree,
  });

  final int running;
  final int parked;
  final HubTotals totals;
  final HubLayout layout;
  final void Function(HubLayout) onLayout;
  final HubFilter filter;
  final void Function(HubFilter) onFilter;
  final AgentTree tree;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    String label(HubFilter f) => switch (f) {
      HubFilter.all => 'All ${tree.nodes.length}',
      HubFilter.running =>
        'Running ${tree.nodes.where((n) => _matches(f, n)).length}',
      HubFilter.done => 'Done ${tree.nodes.where((n) => n.done).length}',
      HubFilter.failed => 'Failed ${tree.nodes.where((n) => n.failed).length}',
    };
    return Container(
      decoration: BoxDecoration(
        color: c.base,
        border: Border(bottom: BorderSide(color: c.borderSubtle)),
      ),
      padding: const EdgeInsets.fromLTRB(
        Space.lg,
        Space.sm,
        Space.lg,
        Space.md,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  '$running running · $parked parked',
                  key: const ValueKey('hub-counts'),
                  style: const TextStyle(
                    fontSize: 15,
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ),
              SegmentedButton<HubLayout>(
                showSelectedIcon: false,
                style: const ButtonStyle(
                  visualDensity: VisualDensity.compact,
                  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                ),
                segments: const [
                  ButtonSegment(value: HubLayout.flat, label: Text('Flat')),
                  ButtonSegment(
                    value: HubLayout.byParent,
                    label: Text('By parent'),
                  ),
                ],
                selected: {layout},
                onSelectionChanged: (s) => onLayout(s.first),
              ),
            ],
          ),
          if (totals.line.isNotEmpty) ...[
            const SizedBox(height: 4),
            Text(
              totals.line,
              key: const ValueKey('hub-totals'),
              style: TextStyle(fontSize: 12.5, color: c.muted),
            ),
          ],
          const SizedBox(height: Space.sm),
          // Four chips share the width: none may hide off the edge of a phone.
          Row(
            children: [
              for (final f in HubFilter.values)
                Expanded(
                  child: Padding(
                    padding: EdgeInsets.only(
                      right: f == HubFilter.values.last ? 0 : Space.xs,
                    ),
                    child: ChoiceChip(
                      key: ValueKey('hub-filter-${f.name}'),
                      label: SizedBox(
                        width: double.infinity,
                        child: Text(
                          label(f),
                          textAlign: TextAlign.center,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                      selected: filter == f,
                      visualDensity: VisualDensity.compact,
                      showCheckmark: false,
                      labelPadding: EdgeInsets.zero,
                      padding: EdgeInsets.zero,
                      onSelected: (_) => onFilter(f),
                    ),
                  ),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

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
      padding: EdgeInsets.only(left: depth * 16.0, top: 6),
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
                        style: const TextStyle(
                          fontSize: 14.5,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                      if (showParent && parent != null)
                        Text(
                          '↳ ${parent.isRoot ? 'Main' : parent.id}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: 12, color: c.faint),
                        ),
                      if (meta.isNotEmpty)
                        Text(
                          meta,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: 12, color: c.muted),
                        ),
                      if (hubTask(n).isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            hubTask(n),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(fontSize: 13, color: c.text),
                          ),
                        ),
                      if (stats.isNotEmpty || last.isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 4),
                          child: Text(
                            [
                              stats,
                              last,
                            ].where((s) => s.isNotEmpty).join(' · '),
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(fontSize: 12, color: c.faint),
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
          const SizedBox(height: 4),
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
                                style: TextStyle(
                                  fontSize: 12.5,
                                  color: c.muted,
                                ),
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
                                  style: TextStyle(
                                    fontSize: 12.5,
                                    color: c.muted,
                                  ),
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
            Container(
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
                        n.isRoot ? 'Message' : 'Message Main about ${n.label}',
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
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 6),
        child: Row(
          children: [
            Text(prefix, style: TextStyle(fontSize: 12.5, color: c.muted)),
            const SizedBox(width: 6),
            Flexible(
              child: Text(
                label,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 13.5,
                  fontWeight: FontWeight.w600,
                  color: c.accent,
                ),
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
              style: TextStyle(
                fontSize: 11.5,
                color: c.faint,
                fontFamily: monoFamily,
                fontFamilyFallback: monoFallback,
              ),
            ),
          ),
          Icon(icon, size: 14, color: color),
          const SizedBox(width: 6),
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
              style: const TextStyle(fontSize: 12.5, height: 1.3),
            ),
          ),
        ],
      ),
    );
  }
}

/// `292K/1M  29%`: how full the context window is.
class ContextBar extends StatelessWidget {
  const ContextBar({super.key, required this.tokens, required this.window});
  final int tokens;
  final int window;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final share = window <= 0 ? 0.0 : (tokens / window).clamp(0.0, 1.0);
    final pct = (share * 100).round();
    final color = share >= .9
        ? c.error
        : share >= .7
        ? c.warning
        : c.accent;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'context ${windowText(tokens)}/${windowText(window)}  $pct%',
          key: const ValueKey('context-label'),
          style: TextStyle(fontSize: 12.5, color: c.muted),
        ),
        const SizedBox(height: 4),
        ClipRRect(
          borderRadius: BorderRadius.circular(3),
          child: LinearProgressIndicator(
            key: const ValueKey('context-bar'),
            value: share,
            minHeight: 6,
            color: color,
            backgroundColor: c.borderSubtle,
          ),
        ),
      ],
    );
  }
}
