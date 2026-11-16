import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/agents/chat/capped_strips.dart';
import 'package:atlas_mobile/features/agents/hub/agent_detail.dart';
import 'package:atlas_mobile/features/agents/tree/breadcrumb.dart';
import 'package:atlas_mobile/features/agents/tree/crew_rail.dart';
import 'package:atlas_mobile/features/agents/tree/jobs_strip.dart';
import 'package:atlas_mobile/features/agents/tree/now_strip.dart';
import 'package:atlas_mobile/features/agents/tree/todo_card.dart';

/// Crew, breadcrumb, jobs, todos and the live line sit above the timeline. On a short
/// phone, with large text or the keyboard up, they must not take the whole screen: they
/// scroll inside a capped area and the timeline keeps the rest.
class ChatStrips extends StatelessWidget {
  const ChatStrips({
    super.key,
    required this.bodyHeight,
    required this.treeNotifier,
    required this.focus,
    required this.status,
    required this.onFocus,
    this.onMessage,
  });

  final double bodyHeight;

  /// The crew; the detail sheet listens to it, so it stays live while open.
  final ValueNotifier<AgentTree> treeNotifier;

  /// The agent the timeline shows (`''` = the main one).
  final String focus;
  final AgentStatus status;
  final void Function(String id) onFocus;

  /// Starts a message about that agent; null hides the button.
  final void Function(AgentNode node)? onMessage;

  @override
  Widget build(BuildContext context) {
    final tree = treeNotifier.value;
    final focusNode = tree.node(focus);
    return CappedStrips(
      bodyHeight: bodyHeight,
      children: [
        if (tree.subs.isNotEmpty)
          CrewRail(
            tree: tree,
            focus: focus,
            fallbackAgent: status.agent,
            onFocus: onFocus,
            onInspect: (n) => showAgentDetail(
              context,
              tree: treeNotifier,
              id: n.id,
              fallbackAgent: status.agent,
              onOpen: onFocus,
              onJump: (id) => showAgentDetail(
                context,
                tree: treeNotifier,
                id: id,
                fallbackAgent: status.agent,
                onOpen: onFocus,
                onJump: onFocus,
                onMessage: onMessage,
              ),
              onMessage: onMessage,
            ),
          ),
        if (focus.isNotEmpty)
          Breadcrumb(path: breadcrumbPath(tree, focus), onTap: onFocus),
        if (focus.isEmpty) ...[
          JobsStrip(tree: tree, onOpen: onFocus),
          if (tree.node('')?.todos case final todos? when todos.total > 0)
            TodoCard(todos),
        ],
        if (focusNode != null && (focusNode.running || tree.subs.isNotEmpty))
          NowStrip(
            node: focusNode,
            since: focusNode.isRoot
                ? status.stateStartedAt
                : DateTime.now().millisecondsSinceEpoch - focusNode.durationMs,
          ),
      ],
    );
  }
}
