/// How a screen says "this computer is not here": reaching it, can't reach it, or it needs an update.
/// The sentences live here once; the home, the pairing flow, a chat and every `AsyncView` use them.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';

/// What the "reaching" state says under its title.
const reachingMessage =
    'Directly when possible, through an encrypted relay otherwise.';

/// What "can't reach" says under its title.
const unreachableMessage =
    'Check that it is on and awake, and that Atlas is installed there.';

/// The full-screen state of a computer that is not connected. [device] null is the pairing flow, where
/// there is no computer yet ("your computer").
///
/// - [connecting]: reaching it (animated logo, no action).
/// - otherwise: its protocol mismatch when it answered but speaks another protocol (the fix), else
///   "Can't reach" with "Try again" (which asks the connection to try now).
class ReachingView extends ConsumerWidget {
  const ReachingView({super.key, this.device, required this.connecting});
  final MacDevice? device;
  final bool connecting;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final d = device;
    final name = d?.name ?? 'your computer';
    // The computer answered but speaks another protocol: one state with its fix, not "can't reach".
    final mismatch = connecting || d == null
        ? null
        : ref.watch(protocolMismatchProvider(d));
    return EmptyState(
      icon: connecting
          ? AtlasLogo(size: d == null ? 96 : 80, animated: true)
          : null,
      title:
          mismatch?.title ??
          (connecting ? 'Reaching $name…' : "Can't reach $name"),
      message:
          mismatch?.message ??
          (connecting ? reachingMessage : unreachableMessage),
      action: connecting || d == null
          ? null
          : FilledButton(
              onPressed: () => ref.invalidate(connectionNotifierProvider(d)),
              child: const Text('Try again'),
            ),
    );
  }
}

/// The one line over a screen that keeps showing what it has (a chat's history) while its computer is
/// off. Nothing while the computer is connected, or while a protocol mismatch is what the screen says
/// (the `ProtocolBanner` above covers that).
class OfflineStrip extends ConsumerWidget {
  const OfflineStrip({super.key, required this.device, this.suffix = ''});
  final MacDevice device;

  /// What the screen still shows, appended: ` · history shown`.
  final String suffix;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final online = ref.watch(connectionNotifierProvider(device)).value != null;
    if (online || ref.watch(protocolMismatchProvider(device)) != null) {
      return const SizedBox.shrink();
    }
    final c = AtlasColors.of(context);
    return Container(
      width: double.infinity,
      color: c.warning.withValues(alpha: .12),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      child: Row(
        children: [
          Icon(Icons.wifi_off_rounded, size: 14, color: c.warning),
          const SizedBox(width: 6),
          Flexible(
            child: Text(
              '${DeviceOffline(device).message}$suffix',
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 12, color: c.warning),
            ),
          ),
        ],
      ),
    );
  }
}
