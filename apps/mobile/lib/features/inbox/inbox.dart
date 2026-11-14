import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/crew_text.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
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
    Navigator.of(context, rootNavigator: true).push(
      MaterialPageRoute<void>(
        builder: (_) => AgentChatScreen(
          device: device,
          paneKey: pane,
          initial: status,
          title: '${status.agent} · ${project.name}',
        ),
      ),
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

/// The most urgent thing, front and centre, with one obvious action.
class AttentionHero extends StatelessWidget {
  const AttentionHero({super.key, required this.item, required this.onOpen});
  final Attention item;
  final void Function(Attention) onOpen;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (color, icon, kicker, title, detail, action) = switch (item) {
      AgentAttention(:final project, :final status) => (
        c.warning,
        Icons.front_hand_rounded,
        '${status.agent} · ${project.name}',
        status.state == 'blocked'
            ? 'Waiting for your approval'
            : 'Asked you something',
        status.lastMessage ?? status.prompt,
        status.state == 'blocked' ? 'Review' : 'Answer',
      ),
      ServiceAttention(:final project, :final service) => (
        c.error,
        Icons.error_outline_rounded,
        project.name,
        '${service.name} stopped working',
        'Open the project to see its logs and restart it.',
        'View',
      ),
      CrewmateAttention(:final crewmate) => (
        c.warning,
        Icons.front_hand_rounded,
        'Crew · ${crewmate.name}',
        crewmate.pendingApproval != null
            ? 'Waiting for your approval'
            : 'Needs you',
        crewmate.last?.text,
        crewmate.pendingApproval != null ? 'Review' : 'Open',
      ),
    };
    return Semantics(
      button: true,
      label: title,
      child: GestureDetector(
        onTap: () => onOpen(item),
        child: Container(
          padding: const EdgeInsets.all(Space.lg),
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(18),
            border: Border.all(color: color.withValues(alpha: .45)),
            gradient: LinearGradient(
              begin: Alignment.topLeft,
              end: Alignment.bottomRight,
              colors: [color.withValues(alpha: .16), c.base],
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(icon, color: color, size: 18),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      kicker,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        color: color,
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: Space.sm),
              Text(
                title,
                style: Theme.of(context).textTheme.titleLarge
                    ?.copyWith(fontSize: 20),
              ),
              if (detail != null) ...[
                const SizedBox(height: 4),
                Text(
                  detail.replaceAll('\n', ' '),
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: c.muted, height: 1.35),
                ),
              ],
              const SizedBox(height: Space.md),
              Align(
                alignment: Alignment.centerRight,
                child: FilledButton(
                  style: FilledButton.styleFrom(
                    backgroundColor: color,
                    foregroundColor: c.onAccent,
                    minimumSize: const Size(0, 40),
                  ),
                  onPressed: () => onOpen(item),
                  child: Text(action),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// The other things that need you, one line each.
class AttentionMore extends StatelessWidget {
  const AttentionMore({super.key, required this.item, required this.onOpen});
  final Attention item;
  final void Function(Attention) onOpen;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (color, text) = switch (item) {
      AgentAttention(:final project, :final status) => (
        c.warning,
        '${status.agent} needs you · ${project.name}',
      ),
      CrewmateAttention(:final crewmate) => (
        c.warning,
        '${crewmate.name} needs you · Crew',
      ),
      ServiceAttention(:final project, :final service) => (
        c.error,
        '${service.name} failed · ${project.name}',
      ),
    };
    return Padding(
      padding: const EdgeInsets.only(top: Space.sm),
      child: AtlasCard(
        margin: EdgeInsets.zero,
        padding: const EdgeInsets.symmetric(
          horizontal: Space.md,
          vertical: Space.sm + 2,
        ),
        accent: color,
        onTap: () => onOpen(item),
        child: Row(
          children: [
            const SizedBox(width: Space.xs),
            Expanded(child: Text(text, overflow: TextOverflow.ellipsis)),
            Icon(Icons.chevron_right_rounded, color: c.muted),
          ],
        ),
      ),
    );
  }
}

/// Inbox tab: everything that needs you, on every paired computer.
class InboxScreen extends ConsumerStatefulWidget {
  const InboxScreen({super.key});

  @override
  ConsumerState<InboxScreen> createState() => _InboxScreenState();
}

class _InboxScreenState extends ConsumerState<InboxScreen> {
  /// The "seen" mark as it was when the screen opened: unread dots stay put while you
  /// read, and clear on the next visit.
  DateTime? _seenBefore;

  @override
  void initState() {
    super.initState();
    _seenBefore = ref.read(inboxSeenProvider);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) ref.read(inboxSeenProvider.notifier).markSeen();
    });
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final items = ref.watch(inboxProvider);
    final recent = ref.watch(recentActivityProvider);
    void open(Attention a) => openAttention(context, a);
    if (items.isEmpty && recent.isEmpty) {
      return Scaffold(
        body: SafeArea(
          bottom: false,
          child: Padding(
            padding: const EdgeInsets.only(bottom: kFloatingBarScrollPad),
            child: EmptyState(
              icon: Icon(
                Icons.notifications_none_rounded,
                size: 56,
                color: c.faint,
              ),
              title: 'All clear',
              message: 'What your crew and agents finish, and anything that needs you, shows up here.',
            ),
          ),
        ),
      );
    }
    return Scaffold(
      body: SafeArea(
        bottom: false,
        child: ListView(
          padding: const EdgeInsets.only(
            top: Space.md,
            bottom: Space.xxl + kFloatingBarScrollPad,
          ),
          children: [
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Space.lg),
              child: Text(
                'Inbox',
                style: Theme.of(context).textTheme.titleMedium
                    ?.copyWith(fontSize: 20, fontWeight: FontWeight.w600),
              ),
            ),
            if (items.isNotEmpty) ...[
              const Padding(
                padding: EdgeInsets.fromLTRB(Space.lg, Space.lg, Space.lg, 0),
                child: SectionHeader('Needs you'),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: Space.lg),
                child: Column(
                  children: [
                    EnterFade(
                      child: AttentionHero(item: items.first, onOpen: open),
                    ),
                    for (var i = 1; i < items.length; i++)
                      EnterFade(
                        index: i,
                        child: AttentionMore(item: items[i], onOpen: open),
                      ),
                  ],
                ),
              ),
            ],
            if (recent.isNotEmpty) ...[
              const Padding(
                padding: EdgeInsets.fromLTRB(Space.lg, Space.lg, Space.lg, 0),
                child: SectionHeader('Recent'),
              ),
              for (var i = 0; i < recent.length; i++)
                EnterFade(
                  index: i,
                  child: _ActivityRow(
                    item: recent[i],
                    unread:
                        _seenBefore == null ||
                        recent[i].at.isAfter(_seenBefore!),
                    last: i == recent.length - 1,
                  ),
                ),
            ],
          ],
        ),
      ),
    );
  }
}

/// One recent event, Telegram-row style: who, what, when, an unread dot.
class _ActivityRow extends StatelessWidget {
  const _ActivityRow({
    required this.item,
    required this.unread,
    required this.last,
  });
  final Activity item;
  final bool unread;
  final bool last;

  void _open(BuildContext context) => switch (item) {
    CrewActivity(:final device, :final crewmate) => context.push(
      '/crew/${device.nodeId}/${crewmate.id}',
      extra: {'device': device},
    ),
    AgentActivity(:final device, :final project, :final pane, :final status) =>
      openAttention(context, AgentAttention(device, project, pane, status)),
  };

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (avatar, title, line, color) = switch (item) {
      CrewActivity(:final crewmate, :final failed, :final text) => (
        Crewmate(
          seed: crewmate.id,
          look: crewmate.look,
          state: CrewState.idle,
          size: 46,
          animate: false,
        ),
        failed ? '${crewmate.name} failed' : '${crewmate.name} finished',
        crewPreview(text),
        failed ? c.error : c.success,
      ),
      AgentActivity(:final project, :final status) => (
        SizedBox.square(
          dimension: 46,
          child: Center(child: AgentLogo(status.agent, size: 30)),
        ),
        '${status.agent} finished · ${project.name}',
        crewPreview(status.lastMessage ?? status.prompt ?? ''),
        c.success,
      ),
    };
    return InkWell(
      onTap: () => _open(context),
      child: Padding(
        padding: const EdgeInsets.only(left: Space.lg),
        child: Row(
          children: [
            avatar,
            const SizedBox(width: 12),
            Expanded(
              child: Container(
                padding: const EdgeInsets.fromLTRB(0, 11, Space.lg, 11),
                decoration: BoxDecoration(
                  border: last
                      ? null
                      : Border(bottom: BorderSide(color: c.borderSubtle)),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Icon(
                          color == c.error
                              ? Icons.error_rounded
                              : Icons.check_circle_rounded,
                          size: 14,
                          color: color,
                        ),
                        const SizedBox(width: 5),
                        Expanded(
                          child: Text(
                            title,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 15,
                              fontWeight: unread
                                  ? FontWeight.w700
                                  : FontWeight.w500,
                              color: c.text,
                            ),
                          ),
                        ),
                        Text(
                          crewTimeAgo(
                            item.at.millisecondsSinceEpoch ~/ 1000,
                            DateTime.now(),
                          ),
                          style: TextStyle(
                            fontSize: 12.5,
                            color: unread ? c.accent : c.faint,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 3),
                    Row(
                      children: [
                        Expanded(
                          child: Text(
                            line,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 14,
                              height: 1.3,
                              color: c.muted,
                            ),
                          ),
                        ),
                        if (unread) ...[
                          const SizedBox(width: 8),
                          Container(
                            width: 9,
                            height: 9,
                            decoration: BoxDecoration(
                              color: c.accent,
                              shape: BoxShape.circle,
                            ),
                          ),
                        ],
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
