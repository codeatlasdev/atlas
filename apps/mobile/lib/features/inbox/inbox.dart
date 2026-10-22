import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';

/// Paired computers from secure storage.
final pairedDevicesProvider = FutureProvider.autoDispose<List<MacDevice>>(
  (ref) => ref.read(macStorageProvider).loadDevices(),
);

/// One thing on a computer that needs the user, most urgent first.
sealed class Attention {
  const Attention(this.project);
  final ProjectInfo project;
}

class AgentAttention extends Attention {
  const AgentAttention(super.project, this.pane, this.status);
  final String pane;
  final AgentStatus status;
}

class ServiceAttention extends Attention {
  const ServiceAttention(super.project, this.service);
  final ServiceInfo service;
}

/// Attention item for a bot that needs the user (has a pending approval).
class BotAttention {
  const BotAttention({required this.device, required this.bot});
  final MacDevice device;
  final BotListEntry bot;
}

/// All items that need the user across every paired computer.
/// Returns [Attention] (agent/service) and [BotAttention] items together.
final inboxProvider = FutureProvider.autoDispose<List<Object>>((ref) async {
  final devices = await ref.watch(pairedDevicesProvider.future);
  final items = <Object>[];
  for (final device in devices) {
    final projects = ref.watch(projectListProvider(device)).value ?? [];
    final live = ref.watch(agentStatusMapNotifierProvider(device)).value ?? {};
    for (final p in projects) {
      for (final (pane, s0) in p.agents) {
        final s = live[pane] ?? s0;
        if (s.state == 'blocked' || s.state == 'waiting') {
          items.add(AgentAttention(p, pane, s));
        }
      }
      if (p.running) {
        for (final svc in p.services.where((s) => s.state == 'failed')) {
          items.add(ServiceAttention(p, svc));
        }
      }
    }
    // Bot attention: bots with a pending approval.
    final bots = ref.watch(botsProvider(device)).value?.bots ?? [];
    for (final bot in bots) {
      if (bot.pendingApproval != null) {
        items.add(BotAttention(device: device, bot: bot));
      }
    }
  }
  return items;
});

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
                      style: TextStyle(
                        color: color,
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: Space.sm),
              Text(
                title,
                style: Theme.of(context).textTheme.titleLarge
                    ?.copyWith(fontSize: 20),
              ),
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
                    minimumSize: const Size(0, 40),
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
      ServiceAttention(:final project, :final service) => (
        c.error,
        '${service.name} failed · ${project.name}',
      ),
    };
    return Padding(
      padding: const EdgeInsets.only(top: Space.sm),
      child: AtlasCard(
        margin: EdgeInsets.zero,
        padding: const EdgeInsets.symmetric(
          horizontal: Space.md,
          vertical: Space.sm + 2,
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
