import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_payload.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/home/home_screen.dart'
    show friendlyDeviceName;

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

/// What looking for a tapped push's agent found.
sealed class PushLookup {
  const PushLookup();
}

/// The push opens no agent (a crew push, or one from a newer hostd).
final class PushNoAgent extends PushLookup {
  const PushNoAgent();
}

final class PushFound extends PushLookup {
  const PushFound(this.target);
  final PushTarget target;
}

/// Every computer answered and none has the pane: the agent was closed.
final class PushGone extends PushLookup {
  const PushGone();
}

/// No computer has the pane, and [names] could not be asked, so the agent may be on one of them.
final class PushUnreachable extends PushLookup {
  const PushUnreachable(this.names);
  final List<String> names;
}

/// How long one computer gets to answer: a tap must not hang on a Mac that is off.
const _askTimeout = Duration(seconds: 6);

/// Finds the paired computer whose hostd has [p]'s pane (`agent_status_all`). The push carries the
/// pane key only, never which computer sent it, so every paired one is asked at once. A computer
/// that cannot be asked is skipped, but remembered: with no hit, "the agent is gone" is only true
/// if every computer answered ([PushUnreachable] otherwise).
Future<PushLookup> resolvePushTarget(
  ProviderContainer container,
  PushPayload p,
) async {
  final pane = p.pane;
  if (pane == null) return const PushNoAgent();
  final holdDevices = container.listen(pairedDevicesProvider, (_, _) {});
  final List<MacDevice> devices;
  try {
    devices = await container.read(pairedDevicesProvider.future);
  } finally {
    holdDevices.close();
  }
  final unreachable = <String>[];
  final asked = await Future.wait(
    devices.where((d) => !isDemo(d)).map((d) async {
      // The connection is auto-disposed: hold it open while we ask (it may still be connecting).
      final hold = container.listen(connectionNotifierProvider(d), (_, _) {});
      try {
        final mux = await container.read(connectionNotifierProvider(d).future);
        final resp = await mux?.call('agent_status_all').timeout(_askTimeout);
        if (resp == null || resp['ok'] != true) {
          unreachable.add(friendlyDeviceName(d.name));
          return null;
        }
        final panes = (resp['data'] as Map)['panes'] as Map<String, dynamic>;
        final raw = panes[pane];
        if (raw == null) return null;
        return PushTarget(
          d,
          pane,
          AgentStatus.fromJson(raw as Map<String, dynamic>),
        );
      } catch (_) {
        unreachable.add(friendlyDeviceName(d.name));
        return null;
      } finally {
        hold.close();
      }
    }),
  );
  final found = asked.nonNulls.firstOrNull;
  if (found != null) return PushFound(found);
  return unreachable.isEmpty ? const PushGone() : PushUnreachable(unreachable);
}

/// Opens the agent a tapped push is about. Wrap the app's navigator content with it: it watches
/// [pushTapProvider], takes the tap (so a rebuild never opens it twice) and pushes the agent's chat
/// on the root navigator. A push whose agent is gone, or whose computer cannot be reached, opens
/// nothing but says which of the two it is.
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
  /// A short message over whatever is showing (the app's own ScaffoldMessenger); always null, so a
  /// `switch` arm can end with it.
  PushTarget? _say(String text) {
    ScaffoldMessenger.maybeOf(context)
        ?.showSnackBar(SnackBar(content: Text(text)));
    return null;
  }

  Future<void> _open(PushPayload p) async {
    final container = ProviderScope.containerOf(context);
    final lookup = await resolvePushTarget(container, p);
    final nav = widget.navigatorKey.currentState;
    if (nav == null || !mounted) return;
    final target = switch (lookup) {
      PushFound(:final target) => target,
      PushNoAgent() => null,
      PushGone() => _say('That agent has finished'),
      PushUnreachable(:final names) => _say(
        '${names.join(' and ')} ${names.length == 1 ? 'is' : 'are'} not reachable',
      ),
    };
    if (target == null) return;
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
