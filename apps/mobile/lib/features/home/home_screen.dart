import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/home/onboarding.dart';

/// Paired Macs from secure storage.
final pairedDevicesProvider = FutureProvider.autoDispose<List<MacDevice>>(
  (ref) => ref.read(macStorageProvider).loadDevices(),
);

/// Home: onboarding until a Mac is paired, then the paired Macs and their connection.
class HomeScreen extends ConsumerWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final devices = ref.watch(pairedDevicesProvider);
    return Scaffold(
      body: AnimatedSwitcher(
        duration: Motion.slow,
        child: devices.when(
          loading: () => const Center(child: AtlasLogo(size: 56)),
          error: (e, _) => EmptyState(
            title: 'Could not read paired Macs',
            message: '$e',
            action: OutlinedButton(
              onPressed: () => ref.invalidate(pairedDevicesProvider),
              child: const Text('Try again'),
            ),
          ),
          data: (list) =>
              list.isEmpty ? const Onboarding() : _MacList(devices: list),
        ),
      ),
    );
  }
}

class _MacList extends ConsumerWidget {
  const _MacList({required this.devices});
  final List<MacDevice> devices;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
    return CustomScrollView(
      slivers: [
        SliverAppBar.large(
          title: Row(
            children: [
              const AtlasLogo(size: 26),
              const SizedBox(width: Space.sm),
              const Text('Atlas'),
            ],
          ),
          actions: [
            IconButton(
              icon: const Icon(Icons.add_rounded),
              tooltip: 'Pair a computer',
              onPressed: () => context.push('/pair'),
            ),
          ],
        ),
        SliverPadding(
          padding: const EdgeInsets.symmetric(horizontal: Space.lg),
          sliver: SliverList.list(
            children: [
              Padding(
                padding: const EdgeInsets.only(bottom: Space.md, left: 2),
                child: Text(
                  devices.length == 1 ? 'Your computer' : 'Your computers',
                  style: Theme.of(context).textTheme.titleSmall,
                ),
              ),
              for (var i = 0; i < devices.length; i++)
                EnterFade(
                  index: i,
                  child: _MacCard(device: devices[i]),
                ),
              const SizedBox(height: Space.lg),
              Text(
                'Long-press a computer to forget it on this phone. To cut a phone off, use '
                'Revoke in Atlas Desktop → Settings → Mobile.',
                style: TextStyle(color: c.faint, fontSize: 12, height: 1.4),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _MacCard extends ConsumerWidget {
  const _MacCard({required this.device});
  final MacDevice device;

  Future<void> _forget(BuildContext context, WidgetRef ref) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Forget ${device.name}?'),
        content: const Text(
          'This phone stops connecting to it. Pair again with a new code to undo.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              backgroundColor: AtlasColors.of(ctx).error,
            ),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Forget'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    await ref.read(macStorageProvider).removeDevice(device.nodeId);
    ref.invalidate(pairedDevicesProvider);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
    final conn = ref.watch(connectionNotifierProvider(device));
    final connected = conn.value != null;
    final (label, color, pulse) = connected
        ? ('Connected', c.success, false)
        : conn.isLoading
        ? ('Connecting…', c.warning, true)
        : ('Unreachable · is it awake?', c.faint, false);
    return GestureDetector(
      onLongPress: () => _forget(context, ref),
      child: AtlasCard(
        padding: const EdgeInsets.all(Space.lg),
        onTap: connected ? () => context.push('/device', extra: device) : null,
        child: Row(
          children: [
            Container(
              width: 48,
              height: 48,
              decoration: BoxDecoration(
                color: c.raised,
                borderRadius: BorderRadius.circular(Space.radius),
              ),
              child: Icon(Icons.laptop_mac_rounded, color: c.text, size: 26),
            ),
            const SizedBox(width: Space.lg),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    device.name,
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                  const SizedBox(height: 4),
                  Row(
                    children: [
                      StatusDot(color: color, pulse: pulse, size: 7),
                      const SizedBox(width: 2),
                      Flexible(
                        child: AnimatedSwitcher(
                          duration: Motion.normal,
                          child: Text(
                            label,
                            key: ValueKey(label),
                            style: TextStyle(color: c.muted, fontSize: 13),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
            AnimatedOpacity(
              opacity: connected ? 1 : .3,
              duration: Motion.normal,
              child: Icon(Icons.chevron_right_rounded, color: c.muted),
            ),
          ],
        ),
      ),
    );
  }
}
