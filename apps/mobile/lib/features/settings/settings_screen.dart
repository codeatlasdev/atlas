import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_transport.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/host_info.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_provider.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/motion_size.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';

/// Settings tab: the paired computers, pairing another, privacy.
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  Future<void> _forget(
    BuildContext context,
    WidgetRef ref,
    MacDevice device,
  ) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Forget ${friendlyDeviceName(device.name)}?'),
        content: const Text(
          'This phone stops connecting to it. Pair again with a new code to undo. '
          'To cut a phone off for good, use Revoke in Atlas Desktop → Settings → Mobile.',
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
    final demo = ref.watch(demoModeProvider);
    final paired = ref.watch(pairedDevicesProvider);
    final devices = demo
        ? const [demoDevice]
        : paired.value ?? const <MacDevice>[];
    // Storage has not answered yet: one card's room is held (a person with a computer has at least one), so
    // the rows below do not jump down when the list arrives.
    final loading = !demo && paired.isLoading && !paired.hasValue;
    return Scaffold(
      body: SafeArea(
        bottom: false,
        child: ListView(
          padding: const EdgeInsets.fromLTRB(
            Space.lg,
            Space.md,
            Space.lg,
            Space.xxl + kFloatingBarScrollPad,
          ),
          children: [
            Text('Settings', style: Theme.of(context).textTheme.titleLarge),
            const SizedBox(height: Space.lg),
            SectionHeader('Computers', count: devices.length),
            if (loading)
              const AtlasCard(
                child: SizedBox(height: kMinInteractiveDimension),
              ),
            for (final d in devices)
              _ComputerCard(
                device: d,
                onForget: isDemo(d)
                    ? () => ref.read(demoModeProvider.notifier).state = false
                    : () => _forget(context, ref, d),
              ),
            const SizedBox(height: Space.sm),
            ListTile(
              contentPadding: EdgeInsets.zero,
              leading: Icon(Icons.qr_code_scanner_rounded, color: c.muted),
              title: const Text('Pair another computer'),
              onTap: () => context.push('/pair'),
            ),
            const Divider(height: Space.xl),
            // One line on why notifications work or not; no row when there is nothing to say
            // (a build without Firebase config, or no computer connected yet). It opens its room
            // downward when the answer arrives, so nothing above it moves.
            MotionSize(
              child: switch (ref.watch(pushRegistrationProvider).value) {
                final status? => ListTile(
                  contentPadding: EdgeInsets.zero,
                  leading: Icon(
                    Icons.notifications_none_rounded,
                    color: c.muted,
                  ),
                  title: const Text('Notifications'),
                  subtitle: Text(status),
                ),
                null => const SizedBox(width: double.infinity),
              },
            ),
            ListTile(
              contentPadding: EdgeInsets.zero,
              leading: Icon(Icons.privacy_tip_outlined, color: c.muted),
              title: const Text('Privacy'),
              onTap: openPrivacy,
            ),
          ],
        ),
      ),
    );
  }
}

/// One paired computer: its name, whether it is reachable, and the Atlas it runs (with an update when its
/// channel offers one). Forgetting it is in the menu, not a red button on every card.
class _ComputerCard extends ConsumerWidget {
  const _ComputerCard({required this.device, required this.onForget});
  final MacDevice device;
  final VoidCallback onForget;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
    final demo = isDemo(device);
    final conn = ref.watch(connectionNotifierProvider(device));
    final online = conn.value != null;
    final info = online ? ref.watch(hostInfoProvider(device)).value : null;
    final (dot, status) = online
        ? (c.success, 'Connected')
        : conn.isLoading
        ? (c.warning, 'Connecting…')
        : (c.faint, 'Offline');
    final version = info == null ? null : 'Atlas ${info.version}';
    return AtlasCard(
      padding: const EdgeInsets.fromLTRB(
        Space.md,
        Space.md,
        Space.xs,
        Space.md,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 44,
                height: 44,
                decoration: BoxDecoration(
                  color: c.accent.withValues(alpha: .12),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Icon(
                  Icons.laptop_mac_rounded,
                  color: c.accent,
                  size: 22,
                ),
              ),
              const SizedBox(width: Space.md),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      friendlyDeviceName(device.name),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.titleMedium,
                    ),
                    const SizedBox(height: 3),
                    Row(
                      children: [
                        StatusDot(color: dot, size: 7),
                        const SizedBox(width: 6),
                        Flexible(
                          child: Text(
                            [status, ?version].join(' · '),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: t.bodySmall,
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
              PopupMenuButton<String>(
                tooltip: 'More',
                icon: Icon(Icons.more_horiz_rounded, color: c.muted),
                onSelected: (_) => onForget(),
                itemBuilder: (_) => [
                  PopupMenuItem(
                    value: 'forget',
                    child: Text(
                      demo ? 'Leave demo' : 'Forget this computer',
                      style: TextStyle(color: c.error),
                    ),
                  ),
                ],
              ),
            ],
          ),
          if (info != null && info.updateAvailable) ...[
            const SizedBox(height: Space.md),
            Container(
              padding: const EdgeInsets.all(Space.md),
              margin: const EdgeInsets.only(right: Space.sm),
              decoration: BoxDecoration(
                color: c.accent.withValues(alpha: .08),
                borderRadius: BorderRadius.circular(Space.radiusSm),
                border: Border.all(color: c.accent.withValues(alpha: .25)),
              ),
              child: Row(
                children: [
                  Icon(
                    Icons.system_update_alt_rounded,
                    color: c.accent,
                    size: 20,
                  ),
                  const SizedBox(width: Space.sm),
                  Expanded(
                    child: Text(
                      'Atlas ${info.offered} is available',
                      style: t.bodyMedium!.copyWith(color: c.text),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }
}
