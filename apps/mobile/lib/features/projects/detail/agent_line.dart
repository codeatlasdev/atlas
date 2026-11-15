import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';

/// One agent as a row: logo, session title, state, model, time in state, last message.
class ProjectAgentLine extends StatelessWidget {
  const ProjectAgentLine({
    super.key,
    required this.status,
    required this.onTap,
  });
  final AgentStatus status;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final k = c.agentState(status.state);
    final line = status.state == 'working'
        ? (status.prompt ?? '')
        : (status.lastMessage ?? status.prompt ?? '');
    final since = agentActive(status.state) || status.state == 'done'
        ? elapsed(status.stateStartedAt)
        : '';
    final meta = [
      agentStateLabel(status.state),
      ?status.model,
      if (since.isNotEmpty) since,
    ];
    return Semantics(
      button: true,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: Space.md,
            vertical: Space.sm + 2,
          ),
          child: Row(
            children: [
              AgentLogo(status.agent, size: 34),
              const SizedBox(width: Space.md),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      agentTitle(status),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(fontWeight: FontWeight.w600),
                    ),
                    Row(
                      children: [
                        StatusDot(
                          color: k,
                          pulse: status.state == 'working' || needsYou(status),
                          size: 6,
                        ),
                        Flexible(
                          child: Text(
                            meta.join(' · '),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              color: k,
                              fontSize: 12,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ),
                      ],
                    ),
                    if (line.isNotEmpty)
                      Text(
                        line.replaceAll('\n', ' '),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(fontSize: 12, color: c.muted),
                      ),
                  ],
                ),
              ),
              Icon(Icons.chevron_right, color: c.faint),
            ],
          ),
        ),
      ),
    );
  }
}
