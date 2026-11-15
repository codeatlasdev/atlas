import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/inbox/inbox_providers.dart';

/// The most urgent thing, front and centre, with one obvious action.
class AttentionHero extends StatelessWidget {
  const AttentionHero({super.key, required this.item, required this.onOpen});
  final Attention item;
  final void Function(Attention) onOpen;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (color, icon, kicker, title, detail, action) = switch (item) {
      AgentAttention(:final project, :final status) => (
        c.warning,
        Icons.front_hand_rounded,
        '${status.agent} · ${project.name}',
        status.state == 'blocked'
            ? 'Waiting for your approval'
            : 'Asked you something',
        status.lastMessage ?? status.prompt,
        status.state == 'blocked' ? 'Review' : 'Answer',
      ),
      ServiceAttention(:final project, :final service) => (
        c.error,
        Icons.error_outline_rounded,
        project.name,
        '${service.name} stopped working',
        'Open the project to see its logs and restart it.',
        'View',
      ),
      CrewmateAttention(:final crewmate) => (
        c.warning,
        Icons.front_hand_rounded,
        'Crew · ${crewmate.name}',
        crewmate.pendingApproval != null
            ? 'Waiting for your approval'
            : 'Needs you',
        crewmate.last?.text,
        crewmate.pendingApproval != null ? 'Review' : 'Open',
      ),
    };
    return Semantics(
      button: true,
      label: title,
      child: GestureDetector(
        onTap: () => onOpen(item),
        child: Container(
          padding: const EdgeInsets.all(Space.lg),
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(18),
            border: Border.all(color: color.withValues(alpha: .45)),
            gradient: LinearGradient(
              begin: Alignment.topLeft,
              end: Alignment.bottomRight,
              colors: [color.withValues(alpha: .16), c.base],
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(icon, color: color, size: 18),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      kicker,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.bodyMedium!
                          .copyWith(color: color, fontWeight: FontWeight.w600),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: Space.sm),
              Text(title, style: Theme.of(context).textTheme.titleLarge),
              if (detail != null) ...[
                const SizedBox(height: 4),
                Text(
                  detail.replaceAll('\n', ' '),
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: c.muted, height: 1.35),
                ),
              ],
              const SizedBox(height: Space.md),
              Align(
                alignment: Alignment.centerRight,
                child: FilledButton(
                  style: FilledButton.styleFrom(
                    backgroundColor: color,
                    foregroundColor: c.onAccent,
                    minimumSize: const Size(0, 48),
                  ),
                  onPressed: () => onOpen(item),
                  child: Text(action),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// The other things that need you, one line each.
class AttentionMore extends StatelessWidget {
  const AttentionMore({super.key, required this.item, required this.onOpen});
  final Attention item;
  final void Function(Attention) onOpen;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (color, text) = switch (item) {
      AgentAttention(:final project, :final status) => (
        c.warning,
        '${status.agent} needs you · ${project.name}',
      ),
      CrewmateAttention(:final crewmate) => (
        c.warning,
        '${crewmate.name} needs you · Crew',
      ),
      ServiceAttention(:final project, :final service) => (
        c.error,
        '${service.name} failed · ${project.name}',
      ),
    };
    return Padding(
      padding: const EdgeInsets.only(top: Space.sm),
      child: AtlasCard(
        margin: EdgeInsets.zero,
        // 12 + the 24 dp chevron + 12 = 48 dp, the minimum tap target (it was 46).
        padding: const EdgeInsets.symmetric(
          horizontal: Space.md,
          vertical: Space.md,
        ),
        accent: color,
        onTap: () => onOpen(item),
        child: Row(
          children: [
            const SizedBox(width: Space.xs),
            Expanded(child: Text(text, overflow: TextOverflow.ellipsis)),
            Icon(Icons.chevron_right_rounded, color: c.muted),
          ],
        ),
      ),
    );
  }
}
