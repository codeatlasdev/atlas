import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_payload.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';

/// The push the user tapped and the app has not opened yet. The FCM listeners set it (a tap while
/// the app is in the background, or the one that launched it); [PushTapHost] consumes it.
final pushTapProvider = StateProvider<PushPayload?>((ref) => null);

/// Where a plain agent's push leads: the computer that has the pane, and the pane's status now.
class PushTarget {
  const PushTarget(this.device, this.paneKey, this.status);
  final MacDevice device;
  final String paneKey;
  final AgentStatus status;
}

/// How long one computer gets to answer: a tap must not hang on a Mac that is off.
const _askTimeout = Duration(seconds: 6);

/// Finds the paired computer whose hostd has [p]'s pane (`agent_status_all`). The push carries the
/// pane key only, never which computer sent it, so every paired one is asked at once; one that is
/// unreachable is skipped. `null` when the push opens no agent, or the agent is gone.
Future<PushTarget?> resolvePushTarget(
  ProviderContainer container,
  PushPayload p,
) async {
  final pane = p.pane;
  if (pane == null) return null;
  final holdDevices = container.listen(pairedDevicesProvider, (_, _) {});
  final List<MacDevice> devices;
  try {
    devices = await container.read(pairedDevicesProvider.future);
  } finally {
    holdDevices.close();
  }
  final asked = await Future.wait(
    devices.where((d) => !isDemo(d)).map((d) async {
      // The connection is auto-disposed: hold it open while we ask (it may still be connecting).
      final hold = container.listen(connectionNotifierProvider(d), (_, _) {});
      try {
        final mux = await container.read(connectionNotifierProvider(d).future);
        if (mux == null) return null;
        final resp = await mux.call('agent_status_all').timeout(_askTimeout);
        if (resp['ok'] != true) return null;
        final panes = (resp['data'] as Map)['panes'] as Map<String, dynamic>;
        final raw = panes[pane];
        if (raw == null) return null;
        return PushTarget(
          d,
          pane,
          AgentStatus.fromJson(raw as Map<String, dynamic>),
        );
      } catch (_) {
        return null;
      } finally {
        hold.close();
      }
    }),
  );
  return asked.nonNulls.firstOrNull;
}

/// Opens the agent a tapped push is about. Wrap the app's navigator content with it: it watches
/// [pushTapProvider], takes the tap (so a rebuild never opens it twice) and pushes the agent's chat
/// on the root navigator. A push whose agent is gone opens nothing.
class PushTapHost extends ConsumerStatefulWidget {
  const PushTapHost({
    super.key,
    required this.navigatorKey,
    required this.child,
  });
  final GlobalKey<NavigatorState> navigatorKey;
  final Widget child;

  @override
  ConsumerState<PushTapHost> createState() => _PushTapHostState();
}

class _PushTapHostState extends ConsumerState<PushTapHost> {
  Future<void> _open(PushPayload p) async {
    final container = ProviderScope.containerOf(context);
    final target = await resolvePushTarget(container, p);
    final nav = widget.navigatorKey.currentState;
    if (target == null || nav == null || !mounted) return;
    unawaited(
      nav.push(
        MaterialPageRoute<void>(
          builder: (_) => AgentChatScreen(
            device: target.device,
            paneKey: target.paneKey,
            initial: target.status,
            title: p.title.isNotEmpty ? p.title : target.status.agent,
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    ref.listen<PushPayload?>(pushTapProvider, (_, next) {
      if (next == null) return;
      ref.read(pushTapProvider.notifier).state = null;
      unawaited(_open(next));
    });
    return widget.child;
  }
}
