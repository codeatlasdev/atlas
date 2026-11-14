import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';

/// Result of `crewmate_list`: whether the server is a crew server, and its crewmates.
typedef CrewListResult = ({
  bool isCrewServer,
  String hostName,
  List<CrewmateEntry> crewmates,
});

/// One crewmate with where it lives: what the Crew tab lists.
typedef CrewEntry = ({
  CrewmateEntry crewmate,
  MacDevice device,
  String hostName,
  bool online,
});

/// Answers a pending tool approval of a crewmate (`agent_approve` on its pane).
Future<void> answerApproval(
  WidgetRef ref,
  MacDevice device,
  CrewmatePendingApproval approval,
  bool allow,
) async {
  final mux = ref.read(connectionNotifierProvider(device)).value;
  if (mux == null) return;
  await mux.call('agent_approve', {
    'pane': approval.paneKey,
    'req': approval.req,
    'allow': allow,
  });
}

/// Every computer that can host crewmates: the paired Macs (one may be a crew server
/// itself) and the crew servers each Mac's desktop knows, reached through that Mac.
/// In the demo, the demo computer stands in for the paired ones (as on the home).
final crewDevicesProvider = FutureProvider.autoDispose<List<MacDevice>>((
  ref,
) async {
  final macs = ref.watch(demoModeProvider)
      ? const [demoDevice]
      : await ref.watch(pairedDevicesProvider.future);
  final out = <MacDevice>[...macs];
  for (final mac in macs) {
    final mux = ref.watch(connectionNotifierProvider(mac)).value;
    if (mux == null) continue;
    final resp = await mux.call('crew_servers');
    if (resp['ok'] != true) continue; // an older Mac without the relay
    final servers = (resp['data'] as Map)['servers'] as List<dynamic>? ?? [];
    for (final s in servers.cast<Map<dynamic, dynamic>>()) {
      final id = s['id'] as String;
      out.add(MacDevice.crew(mac, id, s['name'] as String? ?? id));
    }
  }
  return out;
});

/// Loads crewmates from a device. Returns isCrewServer=false when crewmate_list errors
/// with the 'not a crew server' message (hostd without ATLAS_CREW_HOST=1).
final crewProvider = AsyncNotifierProvider.autoDispose
    .family<CrewNotifier, CrewListResult, MacDevice>(CrewNotifier.new);

class CrewNotifier extends AsyncNotifier<CrewListResult> {
  CrewNotifier(this.device);
  final MacDevice device;

  @override
  Future<CrewListResult> build() async {
    // Re-fetch on push events for this device.
    ref.listen(devicePushEventsProvider(device), (_, ev) {
      if (!ev.hasValue) return;
      final e = ev.value!;
      final evName = e['ev'] as String? ?? '';
      if (evName == 'crewmate_task' ||
          evName == 'crewmate_updated' ||
          evName.startsWith('agent_status')) {
        ref.invalidateSelf();
      }
    });
    final mux = ref.watch(connectionNotifierProvider(device)).value;
    if (mux == null) {
      return (
        isCrewServer: false,
        hostName: device.name,
        crewmates: const <CrewmateEntry>[],
      );
    }
    final resp = await mux.call('crewmate_list', {});
    if (resp['ok'] != true) {
      return (
        isCrewServer: false,
        hostName: device.name,
        crewmates: const <CrewmateEntry>[],
      );
    }
    final data = (resp['data'] as Map).cast<String, dynamic>();
    final hostName = data['host_name'] as String? ?? device.name;
    final crewmates = [
      for (final b in (data['crewmates'] as List<dynamic>?) ?? const [])
        CrewmateEntry.fromJson((b as Map).cast()),
    ];
    return (isCrewServer: true, hostName: hostName, crewmates: crewmates);
  }
}

/// Whether hostd push [e] is a task change for [crewmateId]. The shape is hostd's real
/// `crewmate_task` event (contract fixture `test/fixtures/crewmate_task_event.json`): a
/// flat `crewmate_id`, not a nested object (reading a nested one never matched, so a
/// crewmate's task list refreshed only in the demo).
bool isTaskPushFor(Map<String, dynamic> e, String crewmateId) =>
    e['ev'] == 'crewmate_task' && e['crewmate_id'] == crewmateId;

/// Loads tasks for a single crewmate.
final crewmateTasksProvider = FutureProvider.autoDispose
    .family<List<CrewmateTask>, (MacDevice, String)>((ref, arg) async {
      final (device, crewmateId) = arg;
      ref.listen(devicePushEventsProvider(device), (_, ev) {
        if (ev.hasValue && isTaskPushFor(ev.value!, crewmateId)) {
          ref.invalidateSelf();
        }
      });
      final mux = ref.watch(connectionNotifierProvider(device)).value;
      if (mux == null) return const [];
      final resp = await mux.call('crewmate_tasks', {
        'id': crewmateId,
        'limit': 50,
      });
      if (resp['ok'] != true) return const [];
      final data = (resp['data'] as Map).cast<String, dynamic>();
      return [
        for (final t in (data['tasks'] as List<dynamic>?) ?? const [])
          CrewmateTask.fromJson((t as Map).cast()),
      ];
    });

/// Accumulates pending computer invites from push events and on-connect fetch.
final computerInvitesProvider =
    StateNotifierProvider<ComputerInvitesNotifier, List<ComputerInvite>>(
      (_) => ComputerInvitesNotifier(),
    );

class ComputerInvitesNotifier extends StateNotifier<List<ComputerInvite>> {
  ComputerInvitesNotifier() : super(const []);

  void add(ComputerInvite invite) {
    if (state.any((i) => i.url == invite.url)) return;
    state = [...state, invite];
  }

  void remove(String url) {
    state = [
      for (final i in state)
        if (i.url != url) i,
    ];
  }
}
