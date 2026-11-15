import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/home/home_screen.dart'
    show friendlyDeviceName;

/// Logo, computer name (tap to switch), and one status pill that says it all.
class HomeHeader extends ConsumerWidget {
  const HomeHeader({
    super.key,
    required this.device,
    required this.conn,
    required this.others,
    required this.onSwitch,
    required this.status,
  });

  final MacDevice device;
  final AsyncValue<Object?> conn;
  final List<MacDevice> others;
  final void Function(MacDevice) onSwitch;

  /// (label, colour) once connected and loaded; null shows the connection state.
  final (String, Color)? status;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
    final mismatch = ref.watch(protocolMismatchProvider(device));
    final (label, color) =
        status ??
        (conn.value != null
            ? ('Connected', c.success)
            : conn.isLoading
            ? ('Connecting…', c.warning)
            : mismatch != null
            ? ('Needs an update', c.warning)
            : ('Unreachable', c.muted));
    final name = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Flexible(
          child: Text(
            friendlyDeviceName(device.name),
            overflow: TextOverflow.ellipsis,
            style: Theme.of(context).textTheme.titleLarge,
          ),
        ),
        if (others.isNotEmpty)
          Icon(Icons.unfold_more_rounded, color: c.muted, size: 20),
      ],
    );
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        Space.lg,
        Space.md,
        Space.xs,
        Space.sm,
      ),
      child: Row(
        children: [
          const AtlasLogo(size: 34),
          const SizedBox(width: Space.md),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (others.isEmpty)
                  name
                else
                  PopupMenuButton<MacDevice>(
                    onSelected: onSwitch,
                    tooltip: 'Switch computer',
                    constraints: const BoxConstraints(minHeight: 48),
                    itemBuilder: (_) => [
                      for (final d in others)
                        PopupMenuItem(
                          value: d,
                          child: Text(
                            d.name,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                    ],
                    child: MinTapTarget(child: name),
                  ),
                const SizedBox(height: 4),
                AnimatedSwitcher(
                  duration: Motion.normal,
                  layoutBuilder: (current, previous) => Stack(
                    alignment: Alignment.centerLeft,
                    children: [...previous, ?current],
                  ),
                  child: AtlasChip(
                    label,
                    key: ValueKey(label),
                    tone: color,
                    leading: ChipDot(
                      pulse: label != 'All clear' && label != 'Connected',
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
