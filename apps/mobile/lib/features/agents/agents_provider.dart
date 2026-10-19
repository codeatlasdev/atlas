import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';

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

    // Keep state live via push events.
    ref.listen<AsyncValue<Map<String, dynamic>>>(
      devicePushEventsProvider(device),
      (_, next) {
        next.whenData((event) {
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
    return null;
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
