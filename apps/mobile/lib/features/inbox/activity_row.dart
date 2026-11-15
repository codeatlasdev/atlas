import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
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
    return Semantics(
      button: true,
      label: '$title. $line',
      excludeSemantics: true,
      onTap: () => _open(context),
      child: InkWell(
        onTap: () => _open(context),
        child: Padding(
          padding: const EdgeInsets.only(left: Space.lg),
          child: Row(
            children: [
              avatar,
              const SizedBox(width: 12),
              Expanded(
                child: Container(
                  padding: const EdgeInsets.fromLTRB(0, 11, Space.lg, 11),
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
                          Icon(
                            color == c.error
                                ? Icons.error_rounded
                                : Icons.check_circle_rounded,
                            size: 14,
                            color: color,
                          ),
                          const SizedBox(width: 5),
                          Expanded(
                            child: Text(
                              title,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 15,
                                fontWeight: unread
                                    ? FontWeight.w700
                                    : FontWeight.w500,
                                color: c.text,
                              ),
                            ),
                          ),
                          Text(
                            crewTimeAgo(
                              item.at.millisecondsSinceEpoch ~/ 1000,
                              DateTime.now(),
                            ),
                            style: TextStyle(
                              fontSize: 12.5,
                              color: unread ? c.accent : c.muted,
                            ),
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
                              style: TextStyle(
                                fontSize: 14,
                                height: 1.3,
                                color: c.muted,
                              ),
                            ),
                          ),
                          if (unread) ...[
                            const SizedBox(width: 8),
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
