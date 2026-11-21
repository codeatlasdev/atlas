import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/inset_row.dart';
import 'package:atlas_mobile/features/projects/project_avatar.dart';

/// A project, Telegram-row style like the crew's logbook: avatar with a running dot, the name
/// and its state on top, under it what its agent is doing (tap: that agent), else its branch. A worktree is
/// named by its branch; [nested] under its project it trades the avatar for a branch mark, indented, so the
/// repository reads as one block. A folder that is not a project has no state: it shows where it is.
class ProjectTile extends StatelessWidget {
  const ProjectTile({
    super.key,
    required this.project,
    required this.agents,
    required this.flagged,
    required this.onOpen,
    this.last = false,
    this.nested = false,
  });

  final ProjectInfo project;
  final List<(String, AgentStatus)> agents;
  final bool flagged;
  final VoidCallback onOpen;

  /// The last row draws no divider under it.
  final bool last;

  /// Drawn under its repository's row (a worktree on the home).
  final bool nested;

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
    final (stateText, stateColor) = !project.isProject && !project.running
        ? ('', c.muted)
        : !project.running
        ? ('stopped', c.muted)
        : failed > 0
        ? ('$failed down', c.error)
        : project.services.isEmpty
        ? ('running', c.success)
        : ('$up/${project.services.length} up', c.success);
    final title = project.worktree
        ? project.branch ?? project.name
        : project.name;
    // What tells this row apart when no agent speaks for it: a worktree's folder, a project's branch, a plain
    // folder's place on disk.
    final idle = !project.isProject
        ? project.root.replaceFirst(RegExp(r'^/(Users|home)/[^/]+'), '~')
        : project.worktree
        ? project.name
        : project.branch;
    return InsetRow(
      semanticsLabel: [
        stateText.isEmpty ? title : '$title, $stateText',
        if (lead != null) '${lead.$2.agent}, ${lead.$2.state}',
      ].join('. '),
      onTap: onOpen,
      last: last,
      // Flies into the project screen's header; the dot says if it runs.
      leading: Stack(
        clipBehavior: Clip.none,
        children: [
          if (nested)
            SizedBox(
              width: 50,
              height: 36,
              child: Align(
                alignment: Alignment.centerRight,
                child: Icon(
                  Icons.subdirectory_arrow_right_rounded,
                  size: 22,
                  color: c.faint,
                ),
              ),
            )
          else
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
      lines: [
        Row(
          children: [
            Expanded(
              child: Text(
                title,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.titleMedium!
                    .copyWith(fontWeight: FontWeight.w600, color: c.text),
              ),
            ),
            Text(
              stateText,
              style: Theme.of(context).textTheme.bodyMedium!
                  .copyWith(color: stateColor),
            ),
          ],
        ),
        const SizedBox(height: 3),
        if (lead == null)
          Text(
            idle ?? (project.running ? 'No agent here' : 'Tap to open'),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: Theme.of(context).textTheme.bodyLarge!
                .copyWith(color: c.muted),
          )
        else
          AgentLine(status: lead.$2, more: agents.length - 1),
      ],
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
        const SizedBox(width: Space.sm),
        Expanded(
          child: Text(
            busy
                ? 'working… $text'
                : asks
                ? 'needs you · $text'
                : text,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: Theme.of(context).textTheme.bodyLarge!.copyWith(
              color: busy
                  ? c.accent
                  : asks
                  ? c.warning
                  : c.muted,
            ),
          ),
        ),
        if (more > 0) ...[
          const SizedBox(width: Space.sm),
          Container(
            padding: const EdgeInsets.symmetric(
              horizontal: Space.sm,
              vertical: 1,
            ),
            decoration: BoxDecoration(
              color: c.raised,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Text(
              '+$more',
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: c.muted),
            ),
          ),
        ],
      ],
    );
  }
}
