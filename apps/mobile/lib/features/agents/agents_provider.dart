import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/agent_approval.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart'
    show noRetryWhenUnavailable;

/// Agents installed on [device] (`agent_integrations`), the ones whose hooks report to
/// Atlas first.
///
/// Throws when it cannot ([DeviceUnavailable] while not connected, a [StateError] with hostd's
/// message when it refuses, an older Atlas without the command included): an empty list means the
/// computer answered "none installed" and nothing else.
final installedAgentsProvider = FutureProvider.autoDispose
    .family<List<AgentIntegration>, MacDevice>(retry: noRetryWhenUnavailable, (
      ref,
      device,
    ) async {
      final mux = await requireMux(ref, device);
      final resp = await mux.call('agent_integrations');
      if (resp['ok'] != true) {
        throw StateError(
          resp['error'] as String? ?? 'Could not read the installed agents',
        );
      }
      final all = [
        for (final a in (resp['data']['agents'] as List?) ?? const [])
          AgentIntegration.fromJson((a as Map).cast<String, dynamic>()),
      ];
      return [
        ...all.where((a) => a.state == 'ok'),
        ...all.where((a) => a.state != 'ok'),
      ];
    });

/// Installed agents whose hooks are off on [device]: their state never reaches Atlas. Throws like
/// [installedAgentsProvider].
final missingIntegrationsProvider = FutureProvider.autoDispose
    .family<List<AgentIntegration>, MacDevice>((ref, device) async {
      final all = await ref.watch(installedAgentsProvider(device).future);
      return all.where((a) => a.state == 'missing').toList();
    });

/// Tracks agent statuses for all panes on [device].
///
/// Initialises from `agent_status_all` then updates on `agent_status` pushes.
final agentStatusMapNotifierProvider = AsyncNotifierProvider.autoDispose
    .family<AgentStatusMapNotifier, Map<String, AgentStatus>, MacDevice>(
      AgentStatusMapNotifier.new,
    );

class AgentStatusMapNotifier extends AsyncNotifier<Map<String, AgentStatus>> {
  AgentStatusMapNotifier(this.device);
  final MacDevice device;

  /// Prompts waiting for an answer, by pane: those `agent_status_all` listed and the approval pushes since.
  /// A chat opened from a screen that already read them shows its banner on its first frame
  /// ([PendingApprovalNotifier] starts from here).
  final approvals = <String, AgentApproval>{};

  @override
  Future<Map<String, AgentStatus>> build() async {
    final mux = await ref.watch(connectionNotifierProvider(device).future);
    if (mux == null) return {};

    final resp = await mux.call('agent_status_all');
    if (resp['ok'] != true) return {};
    final raw = resp['data']['panes'] as Map<String, dynamic>;
    final map = raw.map(
      (k, v) => MapEntry(k, AgentStatus.fromJson(v as Map<String, dynamic>)),
    );
    approvals.clear();
    for (final e in (resp['data']['pending_approvals'] as List?) ?? const []) {
      final m = (e as Map).cast<String, dynamic>();
      final pane = m['pane_key'] as String?;
      if (pane != null) approvals[pane] = AgentApproval.fromJson(m);
    }

    // Keep state live via push events.
    ref.listen<AsyncValue<Map<String, dynamic>>>(
      devicePushEventsProvider(device),
      (_, next) {
        next.whenData((event) {
          final pane = event['pane_key'] as String?;
          if (pane != null && event['ev'] == 'agent_approval') {
            approvals[pane] = AgentApproval.fromJson(event);
          } else if (pane != null && event['ev'] == 'agent_approval_done') {
            approvals.remove(pane);
          }
          if (event['ev'] != 'agent_status') return;
          final paneKey = event['pane_key'] as String;
          final status = AgentStatus.fromJson(
            event['status'] as Map<String, dynamic>,
          );
          final current = state.value ?? {};
          state = AsyncData({...current, paneKey: status});
        });
      },
    );

    return map;
  }
}

/// Tracks the pending approval for a single agent pane.
///
/// Starts from `agent_status_all`'s `pending_approvals` (a prompt pushed before this
/// screen opened), then follows `agent_approval` / `agent_approval_done` pushes.
final pendingApprovalNotifierProvider = NotifierProvider.autoDispose
    .family<PendingApprovalNotifier, AgentApproval?, (MacDevice, String)>(
      PendingApprovalNotifier.new,
    );

class PendingApprovalNotifier extends Notifier<AgentApproval?> {
  PendingApprovalNotifier(this.arg);
  final (MacDevice, String) arg;
  MacDevice get device => arg.$1;
  String get paneKey => arg.$2;

  @override
  AgentApproval? build() {
    ref.watch(connectionNotifierProvider(device));
    ref.listen<AsyncValue<Map<String, dynamic>>>(
      devicePushEventsProvider(device),
      (_, next) {
        next.whenData((event) {
          final ev = event['ev'] as String?;
          final pk = event['pane_key'] as String?;
          if (pk != paneKey) return;
          if (ev == 'agent_approval') {
            state = AgentApproval.fromJson(event);
          } else if (ev == 'agent_approval_done') {
            state = null;
          }
        });
      },
    );
    _seed();
    // Already read by a screen of this computer (the project list, the inbox): there from the first frame.
    final statuses = agentStatusMapNotifierProvider(device);
    return ref.exists(statuses)
        ? ref.read(statuses.notifier).approvals[paneKey]
        : null;
  }

  Future<void> _seed() async {
    final mux = await ref.read(connectionNotifierProvider(device).future);
    if (mux == null || !ref.mounted) return;
    final resp = await mux.call('agent_status_all');
    if (!ref.mounted || resp['ok'] != true || state != null) return;
    for (final e in (resp['data']['pending_approvals'] as List?) ?? const []) {
      final m = (e as Map).cast<String, dynamic>();
      if (m['pane_key'] == paneKey) state = AgentApproval.fromJson(m);
    }
  }
}
