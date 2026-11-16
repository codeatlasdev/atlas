import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/agent_history.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/timeline_items.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';

/// The bar of an agent chat: logo, title and state line (of the main agent, or of the
/// subagent [sub] being read), the agent hub and, on the main agent, the terminal.
///
/// A null [onSwitch] means there is nothing to switch to; a null [onTerminal] hides the
/// button (a subagent has no terminal of its own).
PreferredSizeWidget agentChatAppBar(
  BuildContext context, {
  required AgentStatus status,
  required AgentHistory history,
  required String? sub,
  required VoidCallback? onSwitch,
  required VoidCallback? onHub,
  required VoidCallback? onTerminal,
}) {
  final c = AtlasColors.of(context);
  final title =
      sub?.split('.').last ??
      agentTitle(
        status,
        historyTitle: history.title,
        firstPrompt: _firstPrompt(history),
      );
  final inState = agentActive(status.state)
      ? elapsed(status.stateStartedAt)
      : '';
  final subtitle = sub != null
      ? '${agentLabel(status.agent)} · subagent'
      : [
          agentStateLabel(status.state),
          ?status.model,
          if (inState.isNotEmpty) inState,
        ].join(' · ');
  final canSwitch = onSwitch != null;
  return AppBar(
    titleSpacing: 0,
    // Two lines of text (title, state): the bar grows with the text size instead of
    // clipping the second one.
    toolbarHeight: math.max(
      kToolbarHeight,
      MediaQuery.textScalerOf(context).scale(16 * 1.4 + 12 * 1.4) + 16,
    ),
    title: InkWell(
      borderRadius: BorderRadius.circular(Space.radiusSm),
      onTap: onSwitch,
      child: Padding(
        padding: const EdgeInsets.symmetric(
          vertical: Space.xs,
          horizontal: Space.xs,
        ),
        child: Row(
          children: [
            AgentLogo(status.agent, size: 30),
            const SizedBox(width: Space.sm + 2),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Row(
                    children: [
                      Flexible(
                        child: Text(
                          title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: Theme.of(context).textTheme.titleMedium!
                              .copyWith(fontWeight: FontWeight.w600),
                        ),
                      ),
                      if (canSwitch)
                        Icon(Icons.unfold_more, size: 16, color: c.faint),
                    ],
                  ),
                  Row(
                    children: [
                      if (sub == null)
                        StatusDot(
                          color: c.agentState(status.state),
                          pulse:
                              status.state == 'working' ||
                              status.state == 'blocked',
                          size: 6,
                        ),
                      Flexible(
                        child: Text(
                          subtitle,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: Theme.of(context).textTheme.bodySmall!
                              .copyWith(color: c.muted),
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    ),
    actions: [
      IconButton(
        icon: const Icon(Icons.hub_outlined),
        tooltip: 'Agent hub',
        onPressed: onHub,
      ),
      if (onTerminal != null)
        IconButton(
          icon: const Icon(Icons.terminal),
          tooltip: 'Terminal',
          onPressed: onTerminal,
        ),
    ],
  );
}

String _firstPrompt(AgentHistory history) {
  for (final i in history.items) {
    if (i is UserItem) return i.text;
  }
  return '';
}
