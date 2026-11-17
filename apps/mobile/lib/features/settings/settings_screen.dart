import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/app_info.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/links.dart';
import 'package:atlas_mobile/core/protocol/host_info.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/permissions/permission_ui.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/push/push_provider.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/core/ui/grouped_list.dart';
import 'package:atlas_mobile/core/ui/motion_size.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';

/// Settings tab, in groups: the paired computers (and pairing another), this phone, help, about.
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
    final demo = ref.watch(demoModeProvider);
    final paired = ref.watch(pairedDevicesProvider);
    final devices = demo
        ? const [demoDevice]
        : paired.value ?? const <MacDevice>[];
    // Storage has not answered yet: one row's room is held (a person with a computer has at least one), so
    // the groups below do not jump down when the list arrives.
    final loading = !demo && paired.isLoading && !paired.hasValue;
    final version = ref.watch(appVersionProvider).value;
    return Scaffold(
      body: SafeArea(
        bottom: false,
        child: ListView(
          padding: const EdgeInsets.only(
            bottom: Space.xxl + kFloatingBarScrollPad,
          ),
          children: [
            const TabTitle('Settings'),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Space.lg),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  RowGroup(
                    title: 'Computers',
                    footer: 'This phone talks to each computer directly, end-to-end encrypted.',
                    children: [
                      if (loading) const _ComputerRowPlaceholder(),
                      for (final d in devices)
                        _ComputerRow(
                          device: d,
                          onForget: isDemo(d)
                              ? () =>
                                    ref.read(demoModeProvider.notifier).state =
                                        false
                              : () => _forget(context, ref, d),
                        ),
                      GroupRow(
                        icon: Icons.qr_code_scanner_rounded,
                        // While storage answers the room of one computer is held, so this already reads as the row
                        // after it: a title that changed (and could wrap differently) would move what is below.
                        title: devices.isEmpty && !loading
                            ? 'Pair a computer'
                            : 'Pair another computer',
                        subtitle: 'Atlas Desktop → Settings → Mobile',
                        onTap: () => context.push('/pair'),
                      ),
                    ],
                  ),
                  // What the phone lets Atlas do, with the way to change it. Only the rows that exist on this phone
                  // (no push in a local build, no camera, Local Network is iOS only); the group opens its room
                  // downward when the answers arrive, so nothing above it moves and the groups below glide, never
                  // jump (Reduce motion: one step).
                  const _ThisPhone(),
                  RowGroup(
                    title: 'Help',
                    children: [
                      GroupRow(
                        icon: Icons.menu_book_outlined,
                        title: 'Atlas Mobile guide',
                        subtitle: 'Pairing, agents, approvals, the crew',
                        kind: RowKind.external,
                        onTap: () => openLink(AtlasLinks.pairing),
                      ),
                      GroupRow(
                        icon: Icons.vpn_lock_outlined,
                        title: 'Pairing fails on a VPN',
                        subtitle: 'WARP, Tailscale and other tunnels',
                        kind: RowKind.external,
                        onTap: () => openLink(AtlasLinks.pairingBehindVpn),
                      ),
                      GroupRow(
                        icon: Icons.dns_outlined,
                        title: 'Set up Atlas Crew',
                        subtitle: 'Agents that run on your own server',
                        kind: RowKind.external,
                        onTap: () => openLink(AtlasLinks.crewSetup),
                      ),
                    ],
                  ),
                  RowGroup(
                    title: 'About',
                    children: [
                      GroupRow(
                        icon: Icons.info_outline_rounded,
                        title: 'Atlas Mobile',
                        subtitle: version ?? '…',
                        kind: RowKind.info,
                      ),
                      GroupRow(
                        icon: Icons.privacy_tip_outlined,
                        title: 'Privacy policy',
                        subtitle: 'No account, no tracking',
                        kind: RowKind.external,
                        onTap: () => openLink(AtlasLinks.privacy),
                      ),
                      GroupRow(
                        icon: Icons.code_rounded,
                        title: 'Source code',
                        subtitle: 'Open source, MIT or Apache-2.0',
                        kind: RowKind.external,
                        onTap: () => openLink(AtlasLinks.source),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Settings → This phone: one row per permission this phone has, with its real state and the action that fits
/// it: "Turn on" / "Allow" asks (after the explanation), "Open Settings" is the way back from a refusal.
class _ThisPhone extends ConsumerWidget {
  const _ThisPhone();

  static const _all = [
    AtlasPermission.notifications,
    AtlasPermission.camera,
    AtlasPermission.localNetwork,
  ];

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final push = ref.watch(pushRegistrationProvider).value;
    final rows = <Widget>[
      for (final p in _all)
        if (ref.watch(permissionStateProvider(p)).value case final state?
            when state != PermissionState.unavailable)
          _PermissionRow(permission: p, state: state, push: push),
    ];
    return PermissionResumeRefresh(
      onResume: () {
        for (final p in _all) {
          ref.invalidate(permissionStateProvider(p));
        }
      },
      child: MotionSize(
        child: rows.isEmpty
            ? const SizedBox(width: double.infinity)
            : RowGroup(title: 'This phone', children: rows),
      ),
    );
  }
}

class _PermissionRow extends ConsumerWidget {
  const _PermissionRow({
    required this.permission,
    required this.state,
    required this.push,
  });
  final AtlasPermission permission;
  final PermissionState state;

  /// The line about notifications reaching the computers (null when there is nothing to add).
  final String? push;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final on = state == PermissionState.granted;
    final (icon, title) = switch (permission) {
      AtlasPermission.notifications => (
        on
            ? Icons.notifications_active_outlined
            : Icons.notifications_off_outlined,
        'Notifications',
      ),
      AtlasPermission.camera => (Icons.photo_camera_outlined, 'Camera'),
      _ => (Icons.wifi_rounded, 'Local network'),
    };
    // Local Network cannot be read on iOS: "granted" only means the person was asked.
    final asked = permission == AtlasPermission.localNetwork && on;
    final subtitle = switch ((permission, state)) {
      (AtlasPermission.notifications, PermissionState.granted) => push ?? 'On',
      (AtlasPermission.camera, PermissionState.granted) => 'On',
      (AtlasPermission.localNetwork, PermissionState.granted) => 'Direct on Wi-Fi, if you allowed it. If you chose Don\'t Allow, Atlas uses the relay.',
      (AtlasPermission.notifications, PermissionState.notAsked) => 'Not asked yet. Approvals and finished agents reach you when Atlas is closed.',
      (AtlasPermission.camera, PermissionState.notAsked) =>
        'Not asked yet. Used to scan the pairing code and take photos.',
      (AtlasPermission.localNetwork, PermissionState.notAsked) =>
        'Not asked yet. Atlas uses the relay until you allow it.',
      (AtlasPermission.notifications, PermissionState.blocked) => 'Off. Approvals and finished agents will not reach you while Atlas is closed.',
      (AtlasPermission.camera, PermissionState.blocked) =>
        'Off. Atlas cannot scan the pairing code or take photos.',
      (_, PermissionState.blocked) =>
        'Off. Atlas uses the relay instead of Wi-Fi.',
      _ => 'Restricted on this phone',
    };
    final String? action = switch (state) {
      PermissionState.notAsked =>
        permission == AtlasPermission.notifications ? 'Turn on' : 'Allow',
      PermissionState.blocked => 'Open Settings',
      PermissionState.granted when asked => 'Open Settings',
      _ => null,
    };
    return GroupRow(
      icon: icon,
      title: title,
      subtitle: subtitle,
      kind: RowKind.info,
      trailing: action == null
          ? null
          : TextButton(
              onPressed: state == PermissionState.notAsked
                  ? () => askPermission(context, ref, permission)
                  : ref.read(permissionsProvider).openSettings,
              child: Text(action),
            ),
    );
  }
}

/// One paired computer: its name, whether it is reachable and the Atlas it runs; an update its channel offers
/// is a second row. Forgetting it is in the menu, not a red button on every row.
class _ComputerRow extends ConsumerWidget {
  const _ComputerRow({required this.device, required this.onForget});
  final MacDevice device;
  final VoidCallback onForget;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
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
    return Column(
      children: [
        GroupRow(
          icon: Icons.laptop_mac_rounded,
          title: friendlyDeviceName(device.name),
          subtitle: [status, ?version].join(' · '),
          status: dot,
          kind: RowKind.info,
          trailing: PopupMenuButton<String>(
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
        ),
        if (info != null && info.updateAvailable) ...[
          Divider(
            height: 1,
            thickness: 1,
            indent: GroupRow.textInset,
            color: c.borderSubtle,
          ),
          GroupRow(
            icon: Icons.system_update_alt_rounded,
            title: 'Atlas ${info.offered} is available',
            subtitle: 'On ${friendlyDeviceName(device.name)}',
            kind: RowKind.info,
          ),
        ],
      ],
    );
  }
}

/// The room of one computer row while storage answers: the same row, invisible, so nothing below moves when
/// the real one arrives.
class _ComputerRowPlaceholder extends StatelessWidget {
  const _ComputerRowPlaceholder();

  @override
  Widget build(BuildContext context) => const ExcludeSemantics(
    child: Opacity(
      opacity: 0,
      child: GroupRow(
        icon: Icons.laptop_mac_rounded,
        title: ' ',
        subtitle: ' ',
        kind: RowKind.info,
        trailing: SizedBox.square(dimension: kMinInteractiveDimension),
      ),
    ),
  );
}
