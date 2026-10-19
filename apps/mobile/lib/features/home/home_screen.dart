import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_transport.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/features/home/onboarding.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';
import 'package:atlas_mobile/features/projects/projects_screen.dart';

/// Showing the in-app demo computer instead of the paired ones (not saved).
final demoModeProvider = StateProvider<bool>((ref) => false);

/// Paired computers from secure storage.
final pairedDevicesProvider = FutureProvider.autoDispose<List<MacDevice>>(
  (ref) => ref.read(macStorageProvider).loadDevices(),
);

/// Home: onboarding until a computer is paired, then that computer's dashboard.
class HomeScreen extends ConsumerStatefulWidget {
  const HomeScreen({super.key});

  @override
  ConsumerState<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends ConsumerState<HomeScreen> {
  /// The computer shown; the first paired one until the user switches.
  String? _nodeId;

  @override
  Widget build(BuildContext context) {
    final devices = ref.watch(pairedDevicesProvider);
    return Scaffold(
      body: AnimatedSwitcher(
        duration: Motion.slow,
        child: devices.when(
          loading: () => const Center(child: AtlasLogo(size: 56)),
          error: (e, _) => EmptyState(
            title: 'Could not read paired computers',
            message: '$e',
            action: OutlinedButton(
              onPressed: () => ref.invalidate(pairedDevicesProvider),
              child: const Text('Try again'),
            ),
          ),
          data: (stored) {
            final demo = ref.watch(demoModeProvider);
            final list = demo ? [demoDevice] : stored;
            if (list.isEmpty) return const Onboarding();
            final device = list.firstWhere(
              (d) => d.nodeId == _nodeId,
              orElse: () => list.first,
            );
            return _Dashboard(
              key: ValueKey(device.nodeId),
              device: device,
              others: [
                for (final d in list)
                  if (d.nodeId != device.nodeId) d,
              ],
              onSwitch: (d) => setState(() => _nodeId = d.nodeId),
            );
          },
        ),
      ),
    );
  }
}

/// One thing on the computer that needs the user, most urgent first.
sealed class _Attention {
  const _Attention(this.project);
  final ProjectInfo project;
}

class _AgentAttention extends _Attention {
  const _AgentAttention(super.project, this.pane, this.status);
  final String pane;
  final AgentStatus status;
}

class _ServiceAttention extends _Attention {
  const _ServiceAttention(super.project, this.service);
  final ServiceInfo service;
}

class _Dashboard extends ConsumerStatefulWidget {
  const _Dashboard({
    super.key,
    required this.device,
    required this.others,
    required this.onSwitch,
  });

  final MacDevice device;
  final List<MacDevice> others;
  final void Function(MacDevice) onSwitch;

  @override
  ConsumerState<_Dashboard> createState() => _DashboardState();
}

class _DashboardState extends ConsumerState<_Dashboard> {
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    // Services have no push: poll the project list. Agent states are pushed live.
    _timer = Timer.periodic(
      const Duration(seconds: 5),
      (_) => ref.invalidate(projectListProvider(widget.device)),
    );
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  Future<void> _forget() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Forget ${widget.device.name}?'),
        content: const Text(
          'This phone stops connecting to it. Pair again with a new code to undo. '
          'To cut a phone off for good, use Revoke in Atlas Desktop → Settings → Mobile.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              backgroundColor: AtlasColors.of(ctx).error,
            ),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Forget'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    await ref.read(macStorageProvider).removeDevice(widget.device.nodeId);
    ref.invalidate(pairedDevicesProvider);
  }

  void _openAgent(String pane, AgentStatus status, ProjectInfo project) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => AgentChatScreen(
          device: widget.device,
          paneKey: pane,
          initial: status,
          title: '${status.agent} · ${project.name}',
        ),
      ),
    );
  }

  void _openProject(ProjectInfo p) =>
      context.push('/project', extra: {'device': widget.device, 'project': p});

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final conn = ref.watch(connectionNotifierProvider(widget.device));
    final connected = conn.value != null;
    final projects = connected
        ? ref.watch(projectListProvider(widget.device))
        : null;
    // Live agent states override the list's snapshot as soon as they are pushed.
    final live = connected
        ? ref.watch(agentStatusMapNotifierProvider(widget.device)).value ??
              const {}
        : const <String, AgentStatus>{};

    final list = projects?.value ?? const <ProjectInfo>[];
    final attention = <_Attention>[];
    final agents = <(ProjectInfo, String, AgentStatus)>[];
    for (final p in list) {
      for (final (pane, s0) in p.agents) {
        final s = live[pane] ?? s0;
        if (s.state == 'blocked' || s.state == 'waiting') {
          attention.add(_AgentAttention(p, pane, s));
        } else {
          agents.add((p, pane, s));
        }
      }
      if (p.running) {
        for (final svc in p.services.where((s) => s.state == 'failed')) {
          attention.add(_ServiceAttention(p, svc));
        }
      }
    }
    // Working first, then done, then idle.
    int rank(String s) => switch (s) {
      'working' => 0,
      'done' => 1,
      _ => 2,
    };
    agents.sort((a, b) => rank(a.$3.state).compareTo(rank(b.$3.state)));

    return RefreshIndicator(
      color: c.accent,
      edgeOffset: 120,
      onRefresh: () async {
        if (!connected) {
          ref.invalidate(connectionNotifierProvider(widget.device));
        } else {
          ref.invalidate(projectListProvider(widget.device));
        }
      },
      child: CustomScrollView(
        physics: const AlwaysScrollableScrollPhysics(),
        slivers: [
          SliverAppBar.large(
            title: _Title(
              device: widget.device,
              conn: conn,
              others: widget.others,
              onSwitch: widget.onSwitch,
            ),
            actions: [
              PopupMenuButton<String>(
                icon: const Icon(Icons.more_horiz_rounded),
                onSelected: (v) => switch (v) {
                  'pair' => context.push('/pair'),
                  _ => _forget(),
                },
                itemBuilder: (_) => const [
                  PopupMenuItem(
                    value: 'pair',
                    child: Text('Pair another computer'),
                  ),
                  PopupMenuItem(
                    value: 'forget',
                    child: Text('Forget this computer'),
                  ),
                ],
              ),
            ],
          ),
          SliverPadding(
            padding: const EdgeInsets.fromLTRB(
              Space.lg,
              0,
              Space.lg,
              Space.xxl,
            ),
            sliver: SliverList.list(
              children: [
                if (!connected)
                  _Offline(device: widget.device, connecting: conn.isLoading)
                else if (projects!.value == null)
                  const Padding(
                    padding: EdgeInsets.only(top: Space.xxl),
                    child: Center(child: CircularProgressIndicator()),
                  )
                else ...[
                  _Summary(
                    attention: attention.length,
                    working: agents
                        .where((a) => a.$3.state == 'working')
                        .length,
                    projects: list,
                  ),
                  if (attention.isNotEmpty) ...[
                    SectionHeader('Needs you', count: attention.length),
                    for (var i = 0; i < attention.length; i++)
                      EnterFade(
                        index: i,
                        child: switch (attention[i]) {
                          _AgentAttention(
                            :final project,
                            :final pane,
                            :final status,
                          ) =>
                            _AttentionCard(
                              color: c.warning,
                              icon: Icons.front_hand_rounded,
                              title: status.state == 'blocked'
                                  ? '${status.agent} needs your approval'
                                  : '${status.agent} asked you something',
                              subtitle: project.name,
                              action: 'Open',
                              onTap: () => _openAgent(pane, status, project),
                            ),
                          _ServiceAttention(:final project, :final service) =>
                            _AttentionCard(
                              color: c.error,
                              icon: Icons.error_outline_rounded,
                              title: '${service.name} failed',
                              subtitle: project.name,
                              action: 'View',
                              onTap: () => _openProject(project),
                            ),
                        },
                      ),
                  ],
                  if (agents.isNotEmpty) ...[
                    SectionHeader('Agents', count: agents.length),
                    for (var i = 0; i < agents.length; i++)
                      EnterFade(
                        index: i,
                        child: _AgentCard(
                          project: agents[i].$1,
                          status: agents[i].$3,
                          onTap: () => _openAgent(
                            agents[i].$2,
                            agents[i].$3,
                            agents[i].$1,
                          ),
                        ),
                      ),
                  ],
                  SectionHeader('Projects', count: list.length),
                  if (list.isEmpty)
                    Text(
                      'Projects you add in Atlas Desktop show up here.',
                      style: TextStyle(color: c.muted),
                    ),
                  for (var i = 0; i < list.length; i++)
                    EnterFade(
                      index: i,
                      child: ProjectCard(
                        device: widget.device,
                        project: list[i],
                      ),
                    ),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// Computer name with its connection state; tap to switch when more than one is paired.
class _Title extends StatelessWidget {
  const _Title({
    required this.device,
    required this.conn,
    required this.others,
    required this.onSwitch,
  });

  final MacDevice device;
  final AsyncValue<Object?> conn;
  final List<MacDevice> others;
  final void Function(MacDevice) onSwitch;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final connected = conn.value != null;
    final (label, color) = connected
        ? ('Connected', c.success)
        : conn.isLoading
        ? ('Connecting…', c.warning)
        : ('Unreachable', c.faint);
    final title = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Flexible(child: Text(device.name, overflow: TextOverflow.ellipsis)),
            if (others.isNotEmpty)
              Icon(Icons.expand_more_rounded, color: c.muted),
          ],
        ),
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            StatusDot(color: color, pulse: conn.isLoading, size: 6),
            Text(
              label,
              style: TextStyle(
                fontSize: 12,
                color: c.muted,
                fontWeight: FontWeight.w400,
              ),
            ),
          ],
        ),
      ],
    );
    if (others.isEmpty) return title;
    return PopupMenuButton<MacDevice>(
      onSelected: onSwitch,
      itemBuilder: (_) => [
        for (final d in others) PopupMenuItem(value: d, child: Text(d.name)),
      ],
      child: title,
    );
  }
}

/// One line of status under the title: all clear, or what is going on.
class _Summary extends StatelessWidget {
  const _Summary({
    required this.attention,
    required this.working,
    required this.projects,
  });

  final int attention;
  final int working;
  final List<ProjectInfo> projects;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final running = projects.where((p) => p.running).length;
    final (icon, color, text) = attention > 0
        ? (
            Icons.notifications_active_rounded,
            c.warning,
            '$attention ${attention == 1 ? 'thing needs' : 'things need'} you',
          )
        : (Icons.check_circle_rounded, c.success, 'All clear');
    final detail = [
      if (working > 0) '$working ${working == 1 ? 'agent' : 'agents'} working',
      '$running of ${projects.length} ${projects.length == 1 ? 'project' : 'projects'} running',
    ].join(' · ');
    return AtlasCard(
      padding: const EdgeInsets.all(Space.lg),
      child: Row(
        children: [
          Container(
            width: 40,
            height: 40,
            decoration: BoxDecoration(
              color: color.withValues(alpha: .15),
              shape: BoxShape.circle,
            ),
            child: Icon(icon, color: color, size: 22),
          ),
          const SizedBox(width: Space.md),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(text, style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: 2),
                Text(detail, style: TextStyle(color: c.muted, fontSize: 13)),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _AttentionCard extends StatelessWidget {
  const _AttentionCard({
    required this.color,
    required this.icon,
    required this.title,
    required this.subtitle,
    required this.action,
    required this.onTap,
  });

  final Color color;
  final IconData icon;
  final String title;
  final String subtitle;
  final String action;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return AtlasCard(
      accent: color,
      onTap: onTap,
      padding: const EdgeInsets.fromLTRB(
        Space.lg,
        Space.md,
        Space.md,
        Space.md,
      ),
      child: Row(
        children: [
          Icon(icon, color: color, size: 20),
          const SizedBox(width: Space.md),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                Text(subtitle, style: TextStyle(color: c.muted, fontSize: 12)),
              ],
            ),
          ),
          Pill(action, color: color),
        ],
      ),
    );
  }
}

class _AgentCard extends StatelessWidget {
  const _AgentCard({
    required this.project,
    required this.status,
    required this.onTap,
  });

  final ProjectInfo project;
  final AgentStatus status;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final line = status.state == 'working'
        ? (status.prompt ?? '')
        : (status.lastMessage ?? status.prompt ?? '');
    return AtlasCard(
      onTap: onTap,
      child: Row(
        children: [
          StatusDot(
            color: c.agentState(status.state),
            pulse: status.state == 'working',
            size: 7,
          ),
          const SizedBox(width: Space.xs),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Text(
                      status.agent,
                      style: const TextStyle(fontWeight: FontWeight.w600),
                    ),
                    Text(
                      ' · ${project.name}',
                      style: TextStyle(color: c.faint, fontSize: 13),
                    ),
                    const Spacer(),
                    Text(
                      agentStateLabel(status.state),
                      style: TextStyle(
                        color: c.agentState(status.state),
                        fontSize: 12,
                      ),
                    ),
                  ],
                ),
                if (line.isNotEmpty)
                  Text(
                    line.replaceAll('\n', ' '),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(color: c.muted, fontSize: 13),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _Offline extends ConsumerWidget {
  const _Offline({required this.device, required this.connecting});
  final MacDevice device;
  final bool connecting;

  @override
  Widget build(BuildContext context, WidgetRef ref) => Padding(
    padding: const EdgeInsets.only(top: Space.xxl),
    child: EmptyState(
      icon: connecting ? const AtlasLogo(size: 80, animated: true) : null,
      title: connecting
          ? 'Reaching ${device.name}…'
          : "Can't reach ${device.name}",
      message: connecting
          ? 'Directly when possible, through an encrypted relay otherwise.'
          : 'Check that it is on and awake, and that Atlas is installed there.',
      action: connecting
          ? null
          : FilledButton(
              onPressed: () =>
                  ref.invalidate(connectionNotifierProvider(device)),
              child: const Text('Try again'),
            ),
    ),
  );
}
