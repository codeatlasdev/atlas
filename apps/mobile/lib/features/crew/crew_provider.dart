import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';

/// Result of `crewmate_list`: whether the server is a crew server, its crewmates, and whether
/// the kill switch is on (`paused`: nothing runs, queues are held).
typedef CrewListResult = ({
  bool isCrewServer,
  String hostName,
  List<CrewmateEntry> crewmates,
  bool paused,
});

/// The `data` of a `crewmate_list` reply. `paused` is strictly a boolean `true`: a crew
/// server older than the kill switch sends nothing and must not look paused.
CrewListResult parseCrewList(Map<String, dynamic> data, String deviceName) => (
  isCrewServer: true,
  hostName: data['host_name'] as String? ?? deviceName,
  crewmates: [
    for (final b in (data['crewmates'] as List<dynamic>?) ?? const [])
      CrewmateEntry.fromJson((b as Map).cast()),
  ],
  paused: data['paused'] == true,
);

/// Whether hostd push [e] changes what `crewmate_list` answers (a task, a profile, a status
/// or the pause state): the Crew tab re-reads the list on these.
bool isCrewListPush(Map<String, dynamic> e) {
  final ev = e['ev'] as String? ?? '';
  return ev == 'crewmate_task' ||
      ev == 'crewmate_updated' ||
      ev == 'crew_paused' ||
      ev.startsWith('agent_status');
}

/// Pauses (cancels running tasks, holds the queues) or resumes a crew server (`crew_pause` /
/// `crew_resume`, `Act` scope). Throws with the server's message when it refuses.
Future<void> setCrewPaused(WidgetRef ref, MacDevice device, bool paused) async {
  final mux = await requireMuxOf(ref, device);
  final resp = await mux.call(paused ? 'crew_pause' : 'crew_resume', {});
  if (resp['ok'] != true) {
    throw StateError(resp['error'] as String? ?? 'the server refused');
  }
  ref.invalidate(crewProvider(device));
}

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
  // The buttons call this without awaiting: a computer that is off leaves the approval pending, as before.
  final Multiplexer mux;
  try {
    mux = await requireMuxOf(ref, device);
  } on DeviceUnavailable {
    return;
  }
  await mux.call('agent_approve', {
    'pane': approval.paneKey,
    'req': approval.req,
    'allow': allow,
  });
}

/// Answers the card a task is parked on (`refused` / `prove_ui`: `retry` or `override`) from the list; the list
/// follows by the `crewmate_task` push. Throws a [StateError] with the server's message when it refuses.
Future<void> answerWaitingAction(
  WidgetRef ref,
  MacDevice device,
  String crewmateId,
  CrewmateWaitingTask task,
  String value,
) async {
  final mux = await requireMuxOf(ref, device);
  final resp = await mux.call('crewmate_action_answer', {
    'id': crewmateId,
    'task': task.id,
    'value': value,
  });
  if (resp['ok'] != true) {
    throw StateError(resp['error'] as String? ?? 'the server refused');
  }
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

/// Riverpod retries a provider that threw (10 times, 0.2 s to 6.4 s) and reports it as loading meanwhile. A
/// computer that is off or on another protocol is not a one-off: the connection already retries on its own
/// backoff and `requireMux` waits for it, so a retry here only keeps the tab on a spinner for about 45 s
/// before it says why (and a protocol mismatch would never be fixed by one). Say it at once.
Duration? noRetryWhenUnavailable(int count, Object error) =>
    error is DeviceUnavailable
    ? null
    : ProviderContainer.defaultRetry(count, error);

/// hostd's own answer to a crew command on a computer that is no crew server (`crew::handle` in
/// crates/atlas-host, the `!hub.crew_host` branch; a crew server reached through a Mac relays it unchanged).
/// The ONE error that means "this computer has no crew"; any other failure is a failure.
const notCrewServerError =
    'this computer is not a crew server (atlas crew install)';

/// Loads crewmates from a device. `isCrewServer: false` only when hostd answers [notCrewServerError] (a hostd
/// without ATLAS_CREW_HOST=1). A computer that is not reachable yet, is offline, speaks another protocol, or
/// refuses is an `AsyncError` ([DeviceUnavailable] or a [StateError] with the server's text), never an empty
/// crew: a screen shows "Reaching…", "Can't reach" or the error, not "No crew yet".
final crewProvider = AsyncNotifierProvider.autoDispose
    .family<CrewNotifier, CrewListResult, MacDevice>(
      CrewNotifier.new,
      retry: noRetryWhenUnavailable,
    );

class CrewNotifier extends AsyncNotifier<CrewListResult> {
  CrewNotifier(this.device);
  final MacDevice device;

  @override
  Future<CrewListResult> build() async {
    // Re-fetch on push events for this device.
    ref.listen(devicePushEventsProvider(device), (_, ev) {
      if (!ev.hasValue) return;
      if (isCrewListPush(ev.value!)) ref.invalidateSelf();
    });
    final mux = await requireMux(ref, device);
    final resp = await mux.call('crewmate_list', {});
    if (resp['ok'] != true) {
      final error = resp['error'] as String?;
      if (error == notCrewServerError) {
        return (
          isCrewServer: false,
          hostName: device.name,
          crewmates: const <CrewmateEntry>[],
          paused: false,
        );
      }
      throw StateError(error ?? 'the server refused');
    }
    return parseCrewList(
      (resp['data'] as Map).cast<String, dynamic>(),
      device.name,
    );
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
    .family<List<CrewmateTask>, (MacDevice, String)>(
      retry: noRetryWhenUnavailable,
      (ref, arg) async {
        final (device, crewmateId) = arg;
        ref.listen(devicePushEventsProvider(device), (_, ev) {
          if (ev.hasValue && isTaskPushFor(ev.value!, crewmateId)) {
            ref.invalidateSelf();
          }
        });
        final mux = await requireMux(ref, device);
        final resp = await mux.call('crewmate_tasks', {
          'id': crewmateId,
          'limit': 50,
        });
        if (resp['ok'] != true) {
          throw StateError(resp['error'] as String? ?? 'the server refused');
        }
        final data = (resp['data'] as Map).cast<String, dynamic>();
        return [
          for (final t in (data['tasks'] as List<dynamic>?) ?? const [])
            CrewmateTask.fromJson((t as Map).cast()),
        ];
      },
    );

/// The bytes of one image sent with a task, fetched from the crew server (`crewmate_attachment`, in pieces) and
/// kept for the session: the thread rebuilds on every push and an image is up to ten round trips. Not
/// auto-disposed, so scrolling a bubble away and back never fetches again; a failure is an `AsyncError` the
/// bubble shows on that image, and `invalidate` retries it.
final taskImageProvider =
    FutureProvider.family<
      LoadedImage,
      (MacDevice device, String crewmateId, String taskId, String name)
    >((ref, arg) async {
      final (device, crewmateId, taskId, name) = arg;
      // Offline: an error the bubble shows with a retry, not a crash or a hole.
      final mux = await requireMux(ref, device);
      return fetchAttachment(
        mux.call,
        crewmateId: crewmateId,
        taskId: taskId,
        name: name,
      );
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

/// An invite to add a crew server, with the computer that holds it (the one to tell when it is dismissed).
typedef PendingInvite = ({MacDevice device, ComputerInvite invite});

/// The invites waiting on every computer this phone reaches (`mobile_invites`, `Read` scope), fetched when the
/// connection comes up. The `computer_invite` push only reaches a phone that is connected at that moment;
/// an invite made while the phone was off or out of range was lost until this existed. A computer that is
/// offline or answers an error adds nothing and never hides the others.
final pendingInvitesProvider = FutureProvider.autoDispose<List<PendingInvite>>((
  ref,
) async {
  final devices = await ref.watch(crewDevicesProvider.future);
  final found = await Future.wait([
    for (final device in devices.where((d) => !isDemo(d)))
      _invitesOf(ref, device),
  ]);
  return [for (final list in found) ...list];
});

Future<List<PendingInvite>> _invitesOf(Ref ref, MacDevice device) async {
  try {
    final mux = await ref.watch(connectionNotifierProvider(device).future);
    if (mux == null) return const [];
    final resp = await mux.call('mobile_invites');
    if (resp['ok'] != true) return const [];
    return [
      for (final i in ((resp['data'] as Map)['invites'] as List?) ?? const [])
        (
          device: device,
          invite: ComputerInvite(
            url: (i as Map)['url'] as String? ?? '',
            name: i['name'] as String? ?? '',
          ),
        ),
    ].where((p) => p.invite.url.isNotEmpty).toList();
  } on Object {
    return const [];
  }
}
