import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/crew_widgets.dart';

/// The whole crew as a vertical graph: one card per agent, connectors down the tree,
/// a live "now" line, numbers, and a header with the progress ("3/5 done · 2 running").
///
/// [tree] is read on every build of the owner, so the map stays live while the chat
/// screen keeps polling; tapping a card returns its id.
class AgentMapScreen extends StatelessWidget {
  const AgentMapScreen({
    super.key,
    required this.tree,
    required this.focus,
    required this.fallbackAgent,
    required this.onFocus,
  });

  final ValueNotifier<AgentTree> tree;
  final String focus;
  final String fallbackAgent;
  final void Function(String id) onFocus;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return ValueListenableBuilder<AgentTree>(
      valueListenable: tree,
      builder: (context, t, _) {
        final flat = t.flat;
        final root = t.node('');
        final total = [
          for (final n in t.nodes)
            if (n.tokens > 0) n,
        ];
        final tokens = total.fold<int>(0, (a, n) => a + n.tokens);
        final cost = total.fold<double>(0, (a, n) => a + n.cost);
        return Scaffold(
          appBar: AppBar(
            title: const Text('Agent map'),
            bottom: PreferredSize(
              preferredSize: const Size.fromHeight(34),
              child: Padding(
                padding: const EdgeInsets.fromLTRB(
                  Space.lg,
                  0,
                  Space.lg,
                  Space.sm,
                ),
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: Text(
                    [
                      if (t.summary.isNotEmpty) t.summary else 'No subagents',
                      if (tokens > 0) '${tokensText(tokens)} tok',
                      if (costText(cost).isNotEmpty) costText(cost),
                    ].join(' · '),
                    style: TextStyle(fontSize: 12.5, color: c.muted),
                  ),
                ),
              ),
            ),
          ),
          body: root == null
              ? Center(
                  child: Text(
                    'Nothing here yet',
                    style: TextStyle(color: c.faint),
                  ),
                )
              : ListView.builder(
                  padding: const EdgeInsets.all(Space.lg),
                  itemCount: flat.length,
                  itemBuilder: (_, i) {
                    final (n, depth) = flat[i];
                    return _MapRow(
                      node: n,
                      depth: depth,
                      last: _isLastChild(t, n),
                      focused: n.id == focus,
                      fallbackAgent: fallbackAgent,
                      onTap: () {
                        Navigator.of(context).pop();
                        onFocus(n.id);
                      },
                    );
                  },
                ),
        );
      },
    );
  }

  bool _isLastChild(AgentTree t, AgentNode n) {
    if (n.isRoot) return true;
    final sibs = t.children(n.parent!);
    return sibs.isEmpty || sibs.last.id == n.id;
  }
}

class _MapRow extends StatelessWidget {
  const _MapRow({
    required this.node,
    required this.depth,
    required this.last,
    required this.focused,
    required this.fallbackAgent,
    required this.onTap,
  });

  final AgentNode node;
  final int depth;
  final bool last;
  final bool focused;
  final String fallbackAgent;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final stats = statsLine(node);
    final dur = durationText(node.durationMs);
    final color = nodeColor(c, node);
    return IntrinsicHeight(
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (depth > 0)
            SizedBox(
              width: 22.0 * depth,
              child: CustomPaint(
                painter: _Connector(color: c.border, last: last),
              ),
            ),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.only(bottom: Space.sm),
              child: Material(
                color: c.base,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(Space.radius),
                  side: BorderSide(
                    color: focused ? c.accent : c.borderSubtle,
                    width: focused ? 1.5 : 1,
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
                        NodeRing(node: node, fallbackAgent: fallbackAgent),
                        const SizedBox(width: Space.md),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Row(
                                children: [
                                  Flexible(
                                    child: Text(
                                      node.isRoot ? 'Main agent' : node.label,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: const TextStyle(
                                        fontWeight: FontWeight.w600,
                                        fontSize: 14,
                                      ),
                                    ),
                                  ),
                                  const SizedBox(width: Space.sm),
                                  Text(
                                    [
                                      nodeStatusLabel(node),
                                      if (dur.isNotEmpty) dur,
                                    ].join(' · '),
                                    style: TextStyle(
                                      fontSize: 12,
                                      fontWeight: FontWeight.w600,
                                      color: color,
                                    ),
                                  ),
                                ],
                              ),
                              if (node.running && node.now != null) ...[
                                const SizedBox(height: 2),
                                Text(
                                  node.now!,
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 12.5,
                                    color: c.text,
                                  ),
                                ),
                              ] else if ((node.task ?? '').isNotEmpty) ...[
                                const SizedBox(height: 2),
                                Text(
                                  node.task!,
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    fontSize: 12.5,
                                    color: c.muted,
                                  ),
                                ),
                              ],
                              if (stats.isNotEmpty) ...[
                                const SizedBox(height: 4),
                                Text(
                                  stats,
                                  style: TextStyle(
                                    fontSize: 11.5,
                                    color: c.faint,
                                  ),
                                ),
                              ],
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// `│` down the left edge and a `─` into the card; the vertical stops at the elbow for
/// the last child.
class _Connector extends CustomPainter {
  _Connector({required this.color, required this.last});
  final Color color;
  final bool last;

  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = color
      ..strokeWidth = 2;
    final x = size.width - 12;
    final mid = 28.0;
    canvas.drawLine(Offset(x, 0), Offset(x, last ? mid : size.height), p);
    canvas.drawLine(Offset(x, mid), Offset(size.width, mid), p);
  }

  @override
  bool shouldRepaint(_Connector old) => old.color != color || old.last != last;
}
