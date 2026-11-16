import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/route_page.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';

/// One thing on a paired computer that needs the user.
sealed class Attention {
  const Attention(this.device);
  final MacDevice device;
}

class AgentAttention extends Attention {
  const AgentAttention(super.device, this.project, this.pane, this.status);
  final ProjectInfo project;
  final String pane;
  final AgentStatus status;
}

class ServiceAttention extends Attention {
  const ServiceAttention(super.device, this.project, this.service);
  final ProjectInfo project;
  final ServiceInfo service;
}

/// A crewmate waiting on the captain: it asked for approval, or says it needs you.
class CrewmateAttention extends Attention {
  const CrewmateAttention(super.device, this.crewmate);
  final CrewmateEntry crewmate;
}

/// Whether a project agent is waiting on the user.
bool agentNeedsYou(AgentStatus s) =>
    s.state == 'blocked' || s.state == 'waiting';

/// Whether a crewmate is waiting on the user. Either signal counts: a crewmate can say
/// `needs_you` without a pending approval, and the reverse.
bool crewmateNeedsYou(CrewmateEntry b) =>
    b.state == 'needs_you' || b.pendingApproval != null;

/// What needs the user on one computer, most urgent first: someone waiting on an
/// answer (agents, then crewmates), then things that broke.
List<Attention> attentionFor(
  MacDevice device, {
  required List<ProjectInfo> projects,
  required Map<String, AgentStatus> live,
  required List<CrewmateEntry> crewmates,
}) {
  final agents = <Attention>[];
  final failed = <Attention>[];
  for (final p in projects) {
    for (final (pane, s0) in p.agents) {
      final s = live[pane] ?? s0;
      if (agentNeedsYou(s)) agents.add(AgentAttention(device, p, pane, s));
    }
    if (p.running) {
      for (final svc in p.services.where((s) => s.state == 'failed')) {
        failed.add(ServiceAttention(device, p, svc));
      }
    }
  }
  return [
    ...agents,
    for (final b in crewmates)
      if (crewmateNeedsYou(b)) CrewmateAttention(device, b),
    ...failed,
  ];
}

/// Everything that needs the user across every paired computer and the crew servers
/// behind them (crewmates only: a crew server's projects are not the user's).
final inboxProvider = Provider.autoDispose<List<Attention>>((ref) {
  final devices = ref.watch(crewDevicesProvider).value ?? const [];
  return [
    for (final device in devices)
      ...attentionFor(
        device,
        projects: device.via != null
            ? const []
            : ref.watch(projectListProvider(device)).value ?? const [],
        live: device.via != null
            ? const {}
            : ref.watch(agentStatusMapNotifierProvider(device)).value ??
                  const {},
        crewmates: ref.watch(crewProvider(device)).value?.crewmates ?? const [],
      ),
  ];
});

/// Whether the Inbox knows enough to say "All clear": `null` once every source has answered (or failed
/// with something already shown), else what it still waits for or why it cannot say.
///
/// [inboxProvider] and [recentActivityProvider] are plain lists (the tab's badge counts them) and an unread
/// source adds nothing to them, so an empty list alone means "nothing" AND "not heard yet". This is the
/// difference: the computers are being listed, or listing them failed, or a crew list is still loading.
enum InboxUnknown { loading, error }

/// The state of what [inboxProvider] and [recentActivityProvider] are built from: `(null, null)` when settled.
({InboxUnknown? unknown, Object? error}) inboxKnowledge(WidgetRef ref) {
  final devices = ref.watch(crewDevicesProvider);
  if (!devices.hasValue) {
    return devices.hasError
        ? (unknown: InboxUnknown.error, error: devices.error)
        : (unknown: InboxUnknown.loading, error: null);
  }
  var loading = false;
  Object? error;
  for (final device in devices.requireValue) {
    final crew = ref.watch(crewProvider(device));
    if (crew.hasValue) continue;
    if (crew.hasError) {
      error ??= crew.error;
    } else {
      loading = true;
    }
  }
  if (loading) return (unknown: InboxUnknown.loading, error: null);
  if (error != null) return (unknown: InboxUnknown.error, error: error);
  return (unknown: null, error: null);
}

/// Something that happened on a computer: a crewmate finished or failed a task, an agent
/// finished its turn. The Inbox lists these under "Recent", newest first; unlike
/// [Attention], nothing is waiting, it is news.
sealed class Activity {
  const Activity(this.device, this.at);
  final MacDevice device;

  /// When it happened.
  final DateTime at;
}

/// A crewmate's last task ended (`crewmate_list`'s `last`: state done|failed, `ts` in seconds).
class CrewActivity extends Activity {
  const CrewActivity(
    super.device,
    super.at,
    this.crewmate, {
    required this.failed,
  });
  final CrewmateEntry crewmate;
  final bool failed;
  String get text => crewmate.last?.text ?? '';
}

/// A project agent finished its turn (`state_started_at` is milliseconds).
class AgentActivity extends Activity {
  const AgentActivity(
    super.device,
    super.at,
    this.project,
    this.pane,
    this.status,
  );
  final ProjectInfo project;
  final String pane;
  final AgentStatus status;
}

/// How far back "Recent" goes.
const recentWindow = Duration(days: 7);

/// What happened on one computer within [recentWindow] of [now].
List<Activity> activityFor(
  MacDevice device, {
  required List<ProjectInfo> projects,
  required Map<String, AgentStatus> live,
  required List<CrewmateEntry> crewmates,
  required DateTime now,
}) {
  final out = <Activity>[];
  for (final b in crewmates) {
    final last = b.last;
    if (last == null || crewmateNeedsYou(b) || b.state == 'working') continue;
    if (last.state != 'done' && last.state != 'failed') continue;
    out.add(
      CrewActivity(
        device,
        DateTime.fromMillisecondsSinceEpoch(last.ts * 1000),
        b,
        failed: last.state == 'failed',
      ),
    );
  }
  for (final p in projects) {
    for (final (pane, s0) in p.agents) {
      final s = live[pane] ?? s0;
      if (s.state != 'done') continue;
      out.add(
        AgentActivity(
          device,
          DateTime.fromMillisecondsSinceEpoch(s.stateStartedAt),
          p,
          pane,
          s,
        ),
      );
    }
  }
  return [
    for (final a in out)
      if (now.difference(a.at) <= recentWindow) a,
  ];
}

/// Everything recent on every computer, newest first.
final recentActivityProvider = Provider.autoDispose<List<Activity>>((ref) {
  final devices = ref.watch(crewDevicesProvider).value ?? const [];
  final now = DateTime.now();
  return [
    for (final device in devices)
      ...activityFor(
        device,
        projects: device.via != null
            ? const []
            : ref.watch(projectListProvider(device)).value ?? const [],
        live: device.via != null
            ? const {}
            : ref.watch(agentStatusMapNotifierProvider(device)).value ??
                  const {},
        crewmates: ref.watch(crewProvider(device)).value?.crewmates ?? const [],
        now: now,
      ),
  ]..sort((a, b) => b.at.compareTo(a.at));
});

/// When the Inbox was last opened: anything newer is unread (a dot, and the tab's count).
final inboxSeenProvider = NotifierProvider<InboxSeen, DateTime?>(InboxSeen.new);

class InboxSeen extends Notifier<DateTime?> {
  @override
  DateTime? build() {
    ref.read(macStorageProvider).loadInboxSeen().then((v) {
      if (v != null && (state == null || v.isAfter(state!))) state = v;
    });
    return null;
  }

  void markSeen() {
    final now = DateTime.now();
    state = now;
    ref.read(macStorageProvider).saveInboxSeen(now);
  }
}

/// Unread recent items: newer than the last visit (all of them before the first one).
int unreadCount(List<Activity> recent, DateTime? seen) => seen == null
    ? recent.length
    : recent.where((a) => a.at.isAfter(seen)).length;

/// Opens where an inbox item is answered or fixed.
void openAttention(BuildContext context, Attention item) => switch (item) {
  AgentAttention(:final device, :final project, :final pane, :final status) =>
    pushScreen<void>(
      context,
      AgentChatScreen(
        device: device,
        paneKey: pane,
        initial: status,
        title: '${status.agent} · ${project.name}',
      ),
      root: true,
    ),
  ServiceAttention(:final device, :final project) => context.push(
    '/project',
    extra: {'device': device, 'project': project},
  ),
  CrewmateAttention(:final device, :final crewmate) => context.push(
    '/crew/${device.nodeId}/${crewmate.id}',
    extra: {'device': device},
  ),
};
