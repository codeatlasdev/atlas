import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/hub/agent_detail.dart';
import 'package:atlas_mobile/features/agents/hub/hub_filter.dart';
import 'package:atlas_mobile/features/agents/hub/hub_header.dart';
import 'package:atlas_mobile/features/agents/hub/hub_row.dart';
import 'package:atlas_mobile/features/agents/hub/hub_totals.dart';

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
              if (_filter.admits(n)) (n, indent ? d : 0),
          ];
          return LayoutBuilder(
            builder: (context, body) => Column(
              children: [
                // The header is never more than half the body: large text on a short phone
                // scrolls it and leaves the roster its room.
                ConstrainedBox(
                  constraints: BoxConstraints(maxHeight: body.maxHeight * .5),
                  child: SingleChildScrollView(
                    child: HubHeader(
                      running: running,
                      parked: parked,
                      totals: totals,
                      layout: _layout,
                      onLayout: (l) => setState(() => _layout = l),
                      filter: _filter,
                      onFilter: (f) => setState(() => _filter = f),
                      tree: t,
                    ),
                  ),
                ),
                Expanded(
                  child: rows.isEmpty
                      ? EmptyState(
                          title: t.nodes.isEmpty
                              ? 'No agents yet'
                              : 'No agents match this filter',
                          message: t.nodes.isEmpty
                              ? 'Start one from a project.'
                              : 'Pick another filter to see the rest.',
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
            ),
          );
        },
      ),
    );
  }
}
