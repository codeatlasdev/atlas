import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/projects/project_avatar.dart';

/// A project, Telegram-row style like the crew's logbook: avatar with a running dot, the name
/// and its state on top, under it what its agent is doing (tap: that agent) or its services.
class ProjectTile extends StatelessWidget {
  const ProjectTile({
    super.key,
    required this.project,
    required this.agents,
    required this.flagged,
    required this.onOpen,
    this.last = false,
  });

  final ProjectInfo project;
  final List<(String, AgentStatus)> agents;
  final bool flagged;
  final VoidCallback onOpen;

  /// The last row draws no divider under it.
  final bool last;

  /// The agent worth showing: one that needs you, else one working, else the latest.
  (String, AgentStatus)? get _lead {
    if (agents.isEmpty) return null;
    int rank(AgentStatus s) => switch (s.state) {
      'blocked' || 'waiting' => 0,
      'working' => 1,
      _ => 2,
    };
    final sorted = [...agents]
      ..sort((a, b) {
        final r = rank(a.$2).compareTo(rank(b.$2));
        return r != 0 ? r : b.$2.stateStartedAt.compareTo(a.$2.stateStartedAt);
      });
    return sorted.first;
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final up = project.services.where((s) => s.state == 'running').length;
    final failed = project.services.where((s) => s.state == 'failed').length;
    final lead = _lead;
    final (stateText, stateColor) = !project.running
        ? ('stopped', c.muted)
        : failed > 0
        ? ('$failed down', c.error)
        : project.services.isEmpty
        ? ('running', c.success)
        : ('$up/${project.services.length} up', c.success);
    return Semantics(
      button: true,
      label: [
        '${project.name}, $stateText',
        if (lead != null) '${lead.$2.agent}, ${lead.$2.state}',
      ].join('. '),
      excludeSemantics: true,
      onTap: onOpen,
      child: InkWell(
        onTap: onOpen,
        child: Padding(
          padding: const EdgeInsets.only(left: Space.lg),
          child: Row(
            children: [
              // Flies into the project screen's header; the dot says if it runs.
              Stack(
                clipBehavior: Clip.none,
                children: [
                  Hero(
                    tag: 'project-${project.id}',
                    child: ProjectAvatar(name: project.name, size: 50),
                  ),
                  if (project.running)
                    Positioned(
                      right: -2,
                      bottom: -2,
                      child: Container(
                        width: 15,
                        height: 15,
                        decoration: BoxDecoration(
                          color: failed > 0 ? c.error : c.success,
                          shape: BoxShape.circle,
                          border: Border.all(color: c.ground, width: 2.5),
                        ),
                      ),
                    ),
                ],
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Container(
                  padding: const EdgeInsets.fromLTRB(0, 12, Space.lg, 12),
                  decoration: BoxDecoration(
                    border: last
                        ? null
                        : Border(bottom: BorderSide(color: c.borderSubtle)),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Expanded(
                            child: Text(
                              project.name,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 16,
                                fontWeight: FontWeight.w600,
                                color: c.text,
                              ),
                            ),
                          ),
                          Text(
                            stateText,
                            style: TextStyle(fontSize: 13, color: stateColor),
                          ),
                        ],
                      ),
                      const SizedBox(height: 3),
                      if (lead == null)
                        Text(
                          project.running ? 'No agent here' : 'Tap to open',
                          style: TextStyle(fontSize: 14.5, color: c.muted),
                        )
                      else
                        AgentLine(status: lead.$2, more: agents.length - 1),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// What a project's agent is doing, in one line: its logo, its words, its state.
class AgentLine extends StatelessWidget {
  const AgentLine({super.key, required this.status, required this.more});
  final AgentStatus status;
  final int more;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final s = status;
    final busy = s.state == 'working';
    final asks = s.state == 'blocked' || s.state == 'waiting';
    final text = ((busy ? s.prompt : s.lastMessage ?? s.prompt) ?? '')
        .replaceAll('\n', ' ');
    // Not tappable on its own: the whole row opens the overview, where the agents are.
    return Row(
      children: [
        AgentLogo(s.agent, size: 15),
        const SizedBox(width: 6),
        Expanded(
          child: Text(
            busy
                ? 'working… $text'
                : asks
                ? 'needs you · $text'
                : text,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 14.5,
              color: busy
                  ? c.accent
                  : asks
                  ? c.warning
                  : c.muted,
            ),
          ),
        ),
        if (more > 0) ...[
          const SizedBox(width: 6),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
            decoration: BoxDecoration(
              color: c.raised,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Text(
              '+$more',
              style: TextStyle(fontSize: 11.5, color: c.muted),
            ),
          ),
        ],
      ],
    );
  }
}
