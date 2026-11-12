import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_transport.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
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
    final devices = ref.watch(pairedDevicesProvider).value ?? const [];
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
            Text(
              'Settings',
              style: Theme.of(context).textTheme.titleMedium
                  ?.copyWith(fontSize: 20, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: Space.lg),
            SectionHeader('Computers', count: devices.length),
            for (final d in devices)
              AtlasCard(
                child: Row(
                  children: [
                    Icon(Icons.laptop_mac_rounded, color: c.accent),
                    const SizedBox(width: Space.md),
                    Expanded(
                      child: Text(
                        friendlyDeviceName(d.name),
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(fontWeight: FontWeight.w600),
                      ),
                    ),
                    TextButton(
                      style: TextButton.styleFrom(foregroundColor: c.error),
                      onPressed: () => _forget(context, ref, d),
                      child: const Text('Forget'),
                    ),
                  ],
                ),
              ),
            const SizedBox(height: Space.sm),
            ListTile(
              contentPadding: EdgeInsets.zero,
              leading: Icon(Icons.qr_code_scanner_rounded, color: c.muted),
              title: const Text('Pair another computer'),
              onTap: () => context.push('/pair'),
            ),
            const Divider(height: Space.xl),
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
