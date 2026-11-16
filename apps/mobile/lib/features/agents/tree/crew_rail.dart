import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/tree/node_ring.dart';
import 'package:atlas_mobile/features/agents/tree/strip_decoration.dart';

/// The crew: the agent and its subagents as chips in a horizontal rail, children
/// indented behind a connector, the focused one lit. Tap focuses, long-press opens the
/// node's task and report.
class CrewRail extends StatelessWidget {
  const CrewRail({
    super.key,
    required this.tree,
    required this.focus,
    required this.fallbackAgent,
    required this.onFocus,
    required this.onInspect,
  });

  final AgentTree tree;
  final String focus;
  final String fallbackAgent;
  final void Function(String id) onFocus;
  final void Function(AgentNode node) onInspect;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final flat = tree.flat;
    // The chip stacks a name (bodyMedium) over a status (labelSmall): the strip is as tall as
    // those two lines in the theme's own line heights, so it follows the scale and text size.
    final text = Theme.of(context).textTheme;
    final lines = MediaQuery.textScalerOf(context).scale(
      text.bodyMedium!.fontSize! * text.bodyMedium!.height! +
          text.labelSmall!.fontSize! * text.labelSmall!.height!,
    );
    return Container(
      decoration: stripDecoration(c),
      child: SizedBox(
        // 66 dp holds the ring and two short lines; at large text the lines grow.
        height: math.max(66, Space.md + 2 + lines + Space.sm),
        child: ListView.builder(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: Space.sm),
          itemCount: flat.length,
          itemBuilder: (_, i) {
            final (n, depth) = flat[i];
            return _CrewChip(
              key: ValueKey('crew-${n.id}'),
              node: n,
              depth: depth,
              // A connector shows the chip belongs to the one before it.
              linked: depth > 0,
              focused: n.id == focus,
              fallbackAgent: fallbackAgent,
              onTap: () => onFocus(n.id),
              onLongPress: () => onInspect(n),
            );
          },
        ),
      ),
    );
  }
}

class _CrewChip extends StatefulWidget {
  const _CrewChip({
    super.key,
    required this.node,
    required this.depth,
    required this.linked,
    required this.focused,
    required this.fallbackAgent,
    required this.onTap,
    required this.onLongPress,
  });

  final AgentNode node;
  final int depth;
  final bool linked;
  final bool focused;
  final String fallbackAgent;
  final VoidCallback onTap;
  final VoidCallback onLongPress;

  @override
  State<_CrewChip> createState() => _CrewChipState();
}

class _CrewChipState extends State<_CrewChip> {
  AgentNode get node => widget.node;
  int get depth => widget.depth;
  bool get linked => widget.linked;
  bool get focused => widget.focused;
  String get fallbackAgent => widget.fallbackAgent;

  @override
  void didUpdateWidget(_CrewChip old) {
    super.didUpdateWidget(old);
    // Focus moved here (a swipe, the map, a result card): bring it into the rail.
    if (widget.focused && !old.focused) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) {
          Scrollable.ensureVisible(
            context,
            duration: Motion.normal,
            curve: Motion.ease,
            alignment: .5,
          );
        }
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final onTap = widget.onTap;
    final onLongPress = widget.onLongPress;
    final name = node.isRoot
        ? (node.agent ?? fallbackAgent).isEmpty
              ? 'Agent'
              : agentLabel(node.agent ?? fallbackAgent)
        : node.label;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (linked)
          // Depth reads as a longer stub: `└─` for children, `└──` for grandchildren.
          Container(width: 6.0 + 6 * depth, height: 2, color: c.border),
        Semantics(
          button: true,
          selected: focused,
          label: '${node.isRoot ? name : node.id}, ${nodeStatusLabel(node)}',
          child: InkWell(
            borderRadius: BorderRadius.circular(Space.radius),
            onTap: () {
              HapticFeedback.selectionClick();
              onTap();
            },
            onLongPress: () {
              HapticFeedback.mediumImpact();
              onLongPress();
            },
            child: AnimatedContainer(
              duration: Motion.normal,
              curve: Motion.ease,
              margin: const EdgeInsets.symmetric(vertical: Space.sm),
              padding: const EdgeInsets.fromLTRB(Space.xs, 2, Space.md, 2),
              decoration: BoxDecoration(
                color: focused ? c.accent.withValues(alpha: .14) : null,
                border: Border.all(
                  color: focused ? c.accent : Colors.transparent,
                ),
                borderRadius: BorderRadius.circular(Space.radius),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  NodeRing(node: node, fallbackAgent: fallbackAgent, size: 30),
                  const SizedBox(width: Space.sm),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 110),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: Theme.of(context).textTheme.bodyMedium!
                              .copyWith(
                                fontWeight: FontWeight.w600,
                                color: focused ? c.text : c.muted,
                              ),
                        ),
                        Text(
                          nodeStatusLabel(node),
                          style: Theme.of(context).textTheme.labelSmall!
                              .copyWith(color: nodeColor(c, node)),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }
}
