import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/projects/detail/agent_line.dart';
import 'package:atlas_mobile/features/projects/worktrees_section.dart';

/// Every agent of the project, one line each; the ones waiting on you first. A
/// prominent button starts a new one, and "New terminal" shows when [onNewTerminal] is set.
class ProjectAgentsTab extends StatelessWidget {
  const ProjectAgentsTab({
    super.key,
    required this.device,
    required this.projectId,
    required this.agents,
    required this.onNewAgent,
    required this.onNewTerminal,
    required this.onOpenAgent,
  });

  final MacDevice device;
  final String projectId;
  final List<(String, AgentStatus)> agents;
  final VoidCallback onNewAgent;

  /// Null when this computer cannot open a terminal.
  final VoidCallback? onNewTerminal;
  final void Function(String pane, AgentStatus status) onOpenAgent;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final list = [
      ...agents.where((e) => needsYou(e.$2)),
      ...agents.where((e) => !needsYou(e.$2)),
    ];
    return Padding(
      padding: const EdgeInsets.only(top: Space.md),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          FilledButton.icon(
            onPressed: onNewAgent,
            icon: const Icon(Icons.add_rounded, size: 20),
            label: const Text('New agent'),
          ),
          if (onNewTerminal != null) ...[
            const SizedBox(height: Space.sm),
            OutlinedButton.icon(
              key: const Key('new-terminal'),
              onPressed: onNewTerminal,
              icon: const Icon(Icons.terminal_rounded, size: 20),
              label: const Text('New terminal'),
            ),
          ],
          const SizedBox(height: Space.md),
          if (list.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: Space.lg),
              child: Text(
                'No agent running in this project.',
                textAlign: TextAlign.center,
                style: TextStyle(color: c.muted),
              ),
            )
          else
            AtlasCard(
              padding: EdgeInsets.zero,
              child: Column(
                children: [
                  for (final (pane, a) in list)
                    ProjectAgentLine(
                      status: a,
                      onTap: () => onOpenAgent(pane, a),
                    ),
                ],
              ),
            ),
          WorktreesSection(device: device, projectId: projectId),
        ],
      ),
    );
  }
}
