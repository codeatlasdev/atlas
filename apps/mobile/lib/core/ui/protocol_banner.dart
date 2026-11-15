import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// One line per computer whose hostd and this app disagree on the protocol (#302), with the fix;
/// nothing when every one agrees. For the screens that list a computer's state in a row or a strip
/// (the Crew tab, a chat) instead of the full-screen state the home shows.
class ProtocolBanner extends ConsumerWidget {
  const ProtocolBanner({super.key, required this.devices});
  final List<MacDevice> devices;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
    final lines = [
      for (final d in devices) ?ref.watch(protocolMismatchProvider(d))?.line,
    ];
    if (lines.isEmpty) return const SizedBox.shrink();
    return Container(
      width: double.infinity,
      color: c.warning.withValues(alpha: .12),
      padding: const EdgeInsets.symmetric(
        horizontal: Space.lg,
        vertical: Space.sm,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final l in lines)
            Row(
              children: [
                Icon(Icons.system_update_rounded, size: 14, color: c.warning),
                const SizedBox(width: Space.sm),
                Flexible(
                  child: Text(
                    l,
                    style: Theme.of(context).textTheme.bodySmall!
                        .copyWith(color: c.warning),
                  ),
                ),
              ],
            ),
        ],
      ),
    );
  }
}
