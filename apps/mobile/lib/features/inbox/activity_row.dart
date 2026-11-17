import 'package:material_ui/material_ui.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/inset_row.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
import 'package:atlas_mobile/features/crew/crew_text.dart';
import 'package:atlas_mobile/features/inbox/inbox_providers.dart';

/// One recent event, Telegram-row style: who, what, when, an unread dot.
class ActivityRow extends StatelessWidget {
  const ActivityRow({
    super.key,
    required this.item,
    required this.unread,
    required this.last,
  });
  final Activity item;
  final bool unread;
  final bool last;

  void _open(BuildContext context) => switch (item) {
    CrewActivity(:final device, :final crewmate) => context.push(
      '/crew/${device.nodeId}/${crewmate.id}',
      extra: {'device': device},
    ),
    AgentActivity(:final device, :final project, :final pane, :final status) =>
      openAttention(context, AgentAttention(device, project, pane, status)),
  };

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (avatar, title, line, color) = switch (item) {
      CrewActivity(:final crewmate, :final failed, :final text) => (
        Crewmate(
          seed: crewmate.id,
          look: crewmate.look,
          state: CrewState.idle,
          size: 46,
          animate: false,
        ),
        failed ? '${crewmate.name} failed' : '${crewmate.name} finished',
        crewPreview(text),
        failed ? c.error : c.success,
      ),
      AgentActivity(:final project, :final status) => (
        SizedBox.square(
          dimension: 46,
          child: Center(child: AgentLogo(status.agent, size: 30)),
        ),
        '${status.agent} finished · ${project.name}',
        crewPreview(status.lastMessage ?? status.prompt ?? ''),
        c.success,
      ),
    };
    return InsetRow(
      semanticsLabel: '$title. $line',
      onTap: () => _open(context),
      last: last,
      leading: avatar,
      lines: [
        Row(
          children: [
            Icon(
              color == c.error
                  ? Icons.error_rounded
                  : Icons.check_circle_rounded,
              size: 14,
              color: color,
            ),
            const SizedBox(width: Space.xs),
            Expanded(
              child: Text(
                title,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodyLarge!.copyWith(
                  fontWeight: unread ? FontWeight.w700 : FontWeight.w500,
                  color: c.text,
                ),
              ),
            ),
            Text(
              crewTimeAgo(
                item.at.millisecondsSinceEpoch ~/ 1000,
                DateTime.now(),
              ),
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: unread ? c.accent : c.muted),
            ),
          ],
        ),
        const SizedBox(height: 3),
        Row(
          children: [
            Expanded(
              child: Text(
                line,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodyLarge!
                    .copyWith(height: 1.3, color: c.muted),
              ),
            ),
            if (unread) ...[
              const SizedBox(width: Space.sm),
              Container(
                width: 9,
                height: 9,
                decoration: BoxDecoration(
                  color: c.accent,
                  shape: BoxShape.circle,
                ),
              ),
            ],
          ],
        ),
      ],
    );
  }
}
