import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';
import 'package:atlas_mobile/features/bots/new_bot_sheet.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';

/// Lists all bots across every paired computer, grouped by device.
class BotsScreen extends ConsumerWidget {
  const BotsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final devicesAsync = ref.watch(pairedDevicesProvider);
    final c = AtlasColors.of(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Bots'),
        actions: [
          if (devicesAsync.value?.isNotEmpty == true)
            IconButton(
              icon: const Icon(Icons.add_rounded),
              tooltip: 'New bot',
              onPressed: () =>
                  _showNewBot(context, devicesAsync.value!),
            ),
        ],
      ),
      body: devicesAsync.when(
        loading: () =>
            const Center(child: CircularProgressIndicator()),
        error: (e, _) =>
            Center(child: Text('$e', style: TextStyle(color: c.error))),
        data: (devices) {
          if (devices.isEmpty) {
            return const EmptyState(
              title: 'No computers paired',
              message:
                  'Pair a computer first to create bots.',
            );
          }
          return _BotsList(devices: devices);
        },
      ),
    );
  }

  void _showNewBot(BuildContext context, List<MacDevice> devices) {
    showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => NewBotSheet(devices: devices),
    );
  }
}

class _BotsList extends ConsumerWidget {
  const _BotsList({required this.devices});
  final List<MacDevice> devices;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // Check if all devices have loaded and all are empty.
    final allLoaded = devices.every((d) => ref.watch(botsProvider(d)).hasValue);
    final allEmpty =
        allLoaded &&
        devices.every((d) => (ref.watch(botsProvider(d)).value ?? []).isEmpty);

    if (allEmpty) {
      return EmptyState(
        title: 'No bots yet',
        message:
            'Create one on a bot computer from Atlas Desktop or here.',
        action: TextButton.icon(
          icon: const Icon(Icons.add_rounded),
          label: const Text('New bot'),
          onPressed: () => showModalBottomSheet<void>(
            context: context,
            isScrollControlled: true,
            showDragHandle: true,
            builder: (_) => NewBotSheet(devices: devices),
          ),
        ),
      );
    }

    return CustomScrollView(
      slivers: [
        SliverSafeArea(
          bottom: false,
          sliver: SliverPadding(
            padding: const EdgeInsets.fromLTRB(
              Space.lg,
              Space.sm,
              Space.lg,
              Space.xxl,
            ),
            sliver: SliverList.list(
              children: [
                _BotsInbox(devices: devices),
                for (final device in devices)
                  _DeviceBots(device: device, allDevices: devices),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _DeviceBots extends ConsumerWidget {
  const _DeviceBots({required this.device, required this.allDevices});
  final MacDevice device;
  final List<MacDevice> allDevices;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final botsAsync = ref.watch(botsProvider(device));
    final c = AtlasColors.of(context);

    return botsAsync.when(
      loading: () => const Padding(
        padding: EdgeInsets.symmetric(vertical: Space.sm),
        child: LinearProgressIndicator(),
      ),
      error: (e, _) => Padding(
        padding: const EdgeInsets.symmetric(vertical: Space.sm),
        child: Text('Error: $e', style: TextStyle(color: c.error)),
      ),
      data: (bots) {
        if (bots.isEmpty) return const SizedBox.shrink();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SectionHeader(device.name),
            for (var i = 0; i < bots.length; i++)
              EnterFade(
                index: i,
                child: _BotTile(device: device, bot: bots[i]),
              ),
          ],
        );
      },
    );
  }
}

class _BotTile extends StatelessWidget {
  const _BotTile({required this.device, required this.bot});
  final MacDevice device;
  final Bot bot;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (stateLabel, stateColor) = switch (bot.state) {
      'working' => ('working', c.accent),
      'needs_you' => ('needs you', c.warning),
      _ => ('idle', c.muted),
    };

    return AtlasCard(
      onTap: () => context.push(
        '/bots/${device.nodeId}/${bot.id}',
        extra: {'device': device},
      ),
      child: Row(
        children: [
          Icon(Icons.smart_toy_rounded, color: c.muted, size: 18),
          const SizedBox(width: Space.sm),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  bot.name,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                Text(
                  bot.agent,
                  style: TextStyle(color: c.muted, fontSize: 12),
                ),
              ],
            ),
          ),
          Pill(stateLabel, color: stateColor),
        ],
      ),
    );
  }
}

class _BotsInbox extends ConsumerWidget {
  const _BotsInbox({required this.devices});
  final List<MacDevice> devices;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final items = [
      for (final device in devices)
        for (final bot in ref.watch(botsProvider(device)).value ?? [])
          if (bot.pendingApproval != null) BotAttention(device: device, bot: bot),
    ];
    if (items.isEmpty) return const SizedBox.shrink();
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SectionHeader('Needs you', count: items.length),
        for (final item in items)
          AtlasCard(
            accent: c.warning,
            onTap: () => context.push(
              '/bots/${item.device.nodeId}/${item.bot.id}',
              extra: {'device': item.device},
            ),
            child: Row(
              children: [
                const SizedBox(width: Space.xs),
                Expanded(
                  child: Text(
                    '${item.bot.name} needs approval',
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                Icon(Icons.chevron_right_rounded, color: c.muted),
              ],
            ),
          ),
      ],
    );
  }
}
