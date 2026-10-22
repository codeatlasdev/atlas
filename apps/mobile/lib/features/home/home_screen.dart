import 'dart:async';

import 'package:flutter/foundation.dart';
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
import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';
import 'package:atlas_mobile/features/projects/project_avatar.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';

/// While the home is not ready: (stage caption, or null while reading storage).
typedef _Loading = ({String? caption});

/// Debug builds only: show the onboarding on every launch, even with a paired computer.
/// `--dart-define=ATLAS_ALWAYS_ONBOARD=false` turns it off; release builds never do it.
const _alwaysOnboard =
    kDebugMode &&
    bool.fromEnvironment('ATLAS_ALWAYS_ONBOARD', defaultValue: true);

/// Onboarding was finished in this launch (debug: lets the dashboard show afterwards).
final onboardingSeenProvider = StateProvider<bool>((ref) => false);


/// Home: onboarding until a computer is paired, then that computer's dashboard.
class HomeScreen extends ConsumerStatefulWidget {
  const HomeScreen({super.key});

  @override
  ConsumerState<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends ConsumerState<HomeScreen> {
  /// The computer shown; the first paired one until the user switches.
  String? _nodeId;

  /// When the current loading screen appeared: it stays until the logo has formed.
  DateTime? _splashSince;
  Timer? _splashTimer;

  /// The last stage shown, kept while the logo finishes forming.
  String? _caption;

  /// Computers whose dashboard has shown: a later reconnect is handled inside it.
  final _ready = <String>{};

  @override
  void dispose() {
    _splashTimer?.cancel();
    super.dispose();
  }

  /// What the home shows: onboarding, or a computer's dashboard.
  ({bool onboard, MacDevice? device, List<MacDevice> others}) _target(
    List<MacDevice> stored,
  ) {
    final onboard =
        stored.isEmpty ||
        (!ref.watch(onboardingSeenProvider) && _alwaysOnboard);
    if (onboard) return (onboard: true, device: null, others: const []);
    final device = stored.firstWhere(
      (d) => d.nodeId == _nodeId,
      orElse: () => stored.first,
    );
    return (
      onboard: false,
      device: device,
      others: [
        for (final d in stored)
          if (d.nodeId != device.nodeId) d,
      ],
    );
  }

  /// One loading screen from launch until the dashboard has data: the connection and the
  /// first project list are watched here so the logo is not torn down between stages.
  _Loading? _loading(AsyncValue<List<MacDevice>> devices, MacDevice? device) {
    if (devices.isLoading && !devices.hasValue) return (caption: null);
    if (device == null || _ready.contains(device.nodeId)) return null;
    final conn = ref.watch(connectionNotifierProvider(device));
    if (conn.value == null) {
      return conn.isLoading ? (caption: 'Reaching ${device.name}…') : null;
    }
    if (ref.watch(projectListProvider(device)).value == null) {
      return (caption: 'Loading projects…');
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final devices = ref.watch(pairedDevicesProvider);
    final target = devices.hasValue ? _target(devices.value!) : null;
    final loading = devices.hasError ? null : _loading(devices, target?.device);

    // Hold the loading screen until the globe has formed, so it never flashes halfway.
    if (loading != null) {
      _splashSince ??= DateTime.now();
      _caption = loading.caption ?? _caption;
    } else if (_splashSince != null) {
      final left = logoEntrance - DateTime.now().difference(_splashSince!);
      if (left > Duration.zero) {
        _splashTimer ??= Timer(left, () {
          if (mounted) setState(() => _splashSince = _splashTimer = null);
        });
      } else {
        _splashSince = null;
        _splashTimer = null;
      }
    }
    final splash = loading != null || _splashSince != null;
    if (!splash && target?.device != null) _ready.add(target!.device!.nodeId);

    return Scaffold(
      body: AnimatedSwitcher(
        duration: Motion.slow,
        child: splash
            ? _Splash(key: const ValueKey('splash'), caption: _caption)
            : devices.hasError
            ? EmptyState(
                title: 'Could not read paired computers',
                message: '${devices.error}',
                action: OutlinedButton(
                  onPressed: () => ref.invalidate(pairedDevicesProvider),
                  child: const Text('Try again'),
                ),
              )
            : target!.onboard
            ? Onboarding(paired: devices.value!.isNotEmpty)
            : _Dashboard(
                key: ValueKey(target.device!.nodeId),
                device: target.device!,
                others: target.others,
                onSwitch: (d) => setState(() => _nodeId = d.nodeId),
              ),
      ),
    );
  }
}

/// The launch screen: the forming logo with the current stage under it.
class _Splash extends StatelessWidget {
  const _Splash({super.key, this.caption});
  final String? caption;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const AtlasLogo(size: 96, animated: true),
          const SizedBox(height: Space.xl),
          SizedBox(
            height: 20,
            child: AnimatedSwitcher(
              duration: Motion.normal,
              child: Text(
                caption ?? '',
                key: ValueKey(caption),
                style: TextStyle(color: c.muted),
              ),
            ),
          ),
        ],
      ),
    );
  }
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
    final attention = <Attention>[];
    final agentsOf = <String, List<(String, AgentStatus)>>{};
    for (final p in list) {
      for (final (pane, s0) in p.agents) {
        final s = live[pane] ?? s0;
        (agentsOf[p.id] ??= []).add((pane, s));
        if (s.state == 'blocked' || s.state == 'waiting') {
          attention.add(AgentAttention(p, pane, s));
        }
      }
      if (p.running) {
        for (final svc in p.services.where((s) => s.state == 'failed')) {
          attention.add(ServiceAttention(p, svc));
        }
      }
    }
    // Agent prompts first (someone is waiting), then failures.
    attention.sort(
      (a, b) => (a is AgentAttention ? 0 : 1).compareTo(
        b is AgentAttention ? 0 : 1,
      ),
    );
    // Projects that need you, then running, then stopped.
    final flagged = {for (final a in attention) a.project.id};
    final sorted = [...list]
      ..sort((a, b) {
        int rank(ProjectInfo p) => flagged.contains(p.id)
            ? 0
            : p.running
            ? 1
            : 2;
        return rank(a).compareTo(rank(b));
      });
    final missing = connected
        ? ref.watch(missingIntegrationsProvider(widget.device)).value ??
              const <AgentIntegration>[]
        : const <AgentIntegration>[];
    final working = agentsOf.values
        .expand((v) => v)
        .where((a) => a.$2.state == 'working')
        .length;

    // Bot attention items for this computer (bots with pending approval).
    final botAttentionItems = connected
        ? (ref.watch(botsProvider(widget.device)).value ?? [])
            .where((b) => b.pendingApproval != null)
            .map((b) => BotAttention(device: widget.device, bot: b))
            .toList()
        : <BotAttention>[];

    return Stack(
      children: [
        // The app icon's glow, faint, behind the header: Atlas, not a generic list.
        Positioned(
          top: -160,
          left: -80,
          right: -80,
          height: 420,
          child: IgnorePointer(
            child: DecoratedBox(
              decoration: BoxDecoration(
                gradient: RadialGradient(
                  colors: [
                    const Color(0xFF3D5BFF).withValues(alpha: .22),
                    const Color(0xFF3D5BFF).withValues(alpha: 0),
                  ],
                ),
              ),
            ),
          ),
        ),
        RefreshIndicator(
          color: c.accent,
          onRefresh: () async {
            if (!connected) {
              ref.invalidate(connectionNotifierProvider(widget.device));
            } else {
              ref.invalidate(projectListProvider(widget.device));
              ref.invalidate(missingIntegrationsProvider(widget.device));
            }
          },
          child: CustomScrollView(
            physics: const AlwaysScrollableScrollPhysics(),
            slivers: [
              SliverSafeArea(
                bottom: false,
                sliver: SliverToBoxAdapter(
                  child: _Header(
                    device: widget.device,
                    conn: conn,
                    others: widget.others,
                    onSwitch: widget.onSwitch,
                    status: !connected || projects?.value == null
                        ? null
                        : attention.isNotEmpty
                        ? (
                            '${attention.length} ${attention.length == 1 ? 'needs' : 'need'} you',
                            c.warning,
                          )
                        : working > 0
                        ? (
                            '$working ${working == 1 ? 'agent' : 'agents'} working',
                            c.accent,
                          )
                        : ('All clear', c.success),
                    menu: _menu(),
                  ),
                ),
              ),
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(
                  Space.lg,
                  Space.sm,
                  Space.lg,
                  Space.xxl,
                ),
                sliver: SliverList.list(
                  children: [
                    if (!connected)
                      _Offline(
                        device: widget.device,
                        connecting: conn.isLoading,
                      )
                    else ...[
                      if (attention.isNotEmpty) ...[
                        EnterFade(
                          child: AttentionHero(item: attention.first, onOpen: _open),
                        ),
                        for (var i = 1; i < attention.length; i++)
                          EnterFade(
                            index: i,
                            child: AttentionMore(
                              item: attention[i],
                              onOpen: _open,
                            ),
                          ),
                        const SizedBox(height: Space.md),
                      ],
                      for (final ba in botAttentionItems)
                        EnterFade(
                          child: _BotAttentionCard(
                            item: ba,
                            onOpen: _openBot,
                          ),
                        ),
                      for (final a in missing) _IntegrationOff(agent: a),
                      if (missing.isNotEmpty) const SizedBox(height: Space.md),
                      SectionHeader('Projects', count: list.length),
                      if (list.isEmpty)
                        Text(
                          'Projects you add in Atlas Desktop show up here.',
                          style: TextStyle(color: c.muted),
                        ),
                      for (var i = 0; i < sorted.length; i++)
                        EnterFade(
                          index: i,
                          child: _ProjectTile(
                            project: sorted[i],
                            agents: agentsOf[sorted[i].id] ?? const [],
                            flagged: flagged.contains(sorted[i].id),
                            onOpen: () => _openProject(sorted[i]),
                            onAgent: (pane, s) =>
                                _openAgent(pane, s, sorted[i]),
                          ),
                        ),
                    ],
                  ],
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  void _open(Attention a) => switch (a) {
    AgentAttention(:final project, :final pane, :final status) => _openAgent(
      pane,
      status,
      project,
    ),
    ServiceAttention(:final project) => _openProject(project),
  };

  void _openBot(BotAttention ba) {
    context.push(
      '/bots/${ba.device.nodeId}/${ba.bot.id}',
      extra: {'device': ba.device},
    );
  }

  /// Settings, behind a gear: the paired computer on top, then what you can do
  /// with it, in a sheet instead of a bare ⋯ menu.
  Widget _menu() => IconButton(
    icon: const Icon(Icons.settings_outlined),
    tooltip: 'Settings',
    onPressed: () => showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (sheet) {
        final c = AtlasColors.of(sheet);
        Widget row(
          IconData icon,
          String label,
          VoidCallback onTap, {
          Color? color,
        }) => ListTile(
          leading: Icon(icon, color: color ?? c.muted),
          title: Text(label, style: TextStyle(color: color)),
          onTap: () {
            Navigator.pop(sheet);
            onTap();
          },
        );
        return SafeArea(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              ListTile(
                leading: Icon(Icons.laptop_mac_rounded, color: c.accent),
                title: Text(
                  widget.device.name,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                subtitle: const Text('Paired'),
              ),
              const Divider(height: 1),
              row(
                Icons.qr_code_scanner_rounded,
                'Pair another computer',
                () => context.push('/pair'),
              ),
              row(Icons.privacy_tip_outlined, 'Privacy', openPrivacy),
              row(
                Icons.link_off_rounded,
                'Forget this computer',
                _forget,
                color: c.error,
              ),
            ],
          ),
        );
      },
    ),
  );
}

/// Logo, computer name (tap to switch), and one status pill that says it all.
class _Header extends StatelessWidget {
  const _Header({
    required this.device,
    required this.conn,
    required this.others,
    required this.onSwitch,
    required this.status,
    required this.menu,
  });

  final MacDevice device;
  final AsyncValue<Object?> conn;
  final List<MacDevice> others;
  final void Function(MacDevice) onSwitch;

  /// (label, colour) once connected and loaded; null shows the connection state.
  final (String, Color)? status;
  final Widget menu;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (label, color) =
        status ??
        (conn.value != null
            ? ('Connected', c.success)
            : conn.isLoading
            ? ('Connecting…', c.warning)
            : ('Unreachable', c.faint));
    final name = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Flexible(
          child: Text(
            device.name,
            overflow: TextOverflow.ellipsis,
            style: Theme.of(context).textTheme.titleLarge
                ?.copyWith(fontSize: 22, letterSpacing: -.4),
          ),
        ),
        if (others.isNotEmpty)
          Icon(Icons.unfold_more_rounded, color: c.muted, size: 20),
      ],
    );
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        Space.lg,
        Space.md,
        Space.xs,
        Space.sm,
      ),
      child: Row(
        children: [
          const AtlasLogo(size: 34),
          const SizedBox(width: Space.md),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (others.isEmpty)
                  name
                else
                  PopupMenuButton<MacDevice>(
                    onSelected: onSwitch,
                    itemBuilder: (_) => [
                      for (final d in others)
                        PopupMenuItem(value: d, child: Text(d.name)),
                    ],
                    child: name,
                  ),
                const SizedBox(height: 4),
                AnimatedSwitcher(
                  duration: Motion.normal,
                  child: Container(
                    key: ValueKey(label),
                    padding: const EdgeInsets.fromLTRB(2, 2, 10, 2),
                    decoration: BoxDecoration(
                      color: color.withValues(alpha: .12),
                      borderRadius: BorderRadius.circular(999),
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        StatusDot(
                          color: color,
                          pulse: label != 'All clear' && label != 'Connected',
                          size: 6,
                        ),
                        Text(
                          label,
                          style: TextStyle(
                            color: color,
                            fontSize: 12,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
          menu,
        ],
      ),
    );
  }
}

/// A project with its health at a glance (one dot per service) and its agents inside.
class _ProjectTile extends StatelessWidget {
  const _ProjectTile({
    required this.project,
    required this.agents,
    required this.flagged,
    required this.onOpen,
    required this.onAgent,
  });

  final ProjectInfo project;
  final List<(String, AgentStatus)> agents;
  final bool flagged;
  final VoidCallback onOpen;
  final void Function(String pane, AgentStatus s) onAgent;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final up = project.services.where((s) => s.state == 'running').length;
    final caption = !project.running
        ? 'Stopped'
        : project.services.isEmpty
        ? 'Running'
        : '$up of ${project.services.length} services up';
    return AtlasCard(
      padding: EdgeInsets.zero,
      child: Column(
        children: [
          InkWell(
            onTap: onOpen,
            child: Padding(
              padding: const EdgeInsets.all(Space.md),
              child: Row(
                children: [
                  // Flies into the project screen's header.
                  Hero(
                    tag: 'project-${project.id}',
                    child: ProjectAvatar(name: project.name, size: 40),
                  ),
                  const SizedBox(width: Space.md),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          project.name,
                          style: Theme.of(context).textTheme.titleMedium,
                        ),
                        const SizedBox(height: 4),
                        Row(
                          children: [
                            for (final s in project.services.take(12))
                              Container(
                                margin: const EdgeInsets.only(right: 3),
                                width: 14,
                                height: 6,
                                decoration: BoxDecoration(
                                  color: project.running
                                      ? c.serviceState(s.state)
                                      : c.border,
                                  borderRadius: BorderRadius.circular(3),
                                ),
                              ),
                            if (project.services.isNotEmpty)
                              const SizedBox(width: 5),
                            Flexible(
                              child: Text(
                                caption,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  color: flagged ? c.error : c.faint,
                                  fontSize: 12,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                  Icon(Icons.chevron_right_rounded, color: c.faint),
                ],
              ),
            ),
          ),
          for (final (pane, s) in agents) ...[
            Divider(height: 1, color: c.borderSubtle),
            InkWell(
              onTap: () => onAgent(pane, s),
              child: Padding(
                padding: const EdgeInsets.fromLTRB(
                  Space.md,
                  Space.sm,
                  Space.md,
                  Space.sm,
                ),
                child: Row(
                  children: [
                    const SizedBox(width: 10),
                    StatusDot(
                      color: c.agentState(s.state),
                      pulse: s.state == 'working' || s.state == 'blocked',
                      size: 6,
                    ),
                    const SizedBox(width: 4),
                    Text(
                      s.agent,
                      style: const TextStyle(
                        fontWeight: FontWeight.w600,
                        fontSize: 13,
                      ),
                    ),
                    const SizedBox(width: Space.sm),
                    Expanded(
                      child: Text(
                        (s.state == 'working'
                                    ? s.prompt
                                    : s.lastMessage ?? s.prompt)
                                ?.replaceAll('\n', ' ') ??
                            '',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(color: c.muted, fontSize: 13),
                      ),
                    ),
                    const SizedBox(width: Space.sm),
                    Text(
                      agentStateLabel(s.state),
                      style: TextStyle(
                        color: c.agentState(s.state),
                        fontSize: 12,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
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

class _BotAttentionCard extends StatelessWidget {
  const _BotAttentionCard({required this.item, required this.onOpen});
  final BotAttention item;
  final void Function(BotAttention) onOpen;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.only(top: Space.sm),
      child: AtlasCard(
        margin: EdgeInsets.zero,
        padding: const EdgeInsets.symmetric(
          horizontal: Space.md,
          vertical: Space.sm + 2,
        ),
        accent: c.warning,
        onTap: () => onOpen(item),
        child: Row(
          children: [
            const SizedBox(width: Space.xs),
            Expanded(
              child: Text(
                '${item.bot.name} needs approval',
                overflow: TextOverflow.ellipsis,
              ),
            ),
            Icon(Icons.chevron_right_rounded, color: c.muted),
          ],
        ),
      ),
    );
  }
}

/// An installed agent whose hooks are off: its tabs never show here until it is turned on.
class _IntegrationOff extends StatelessWidget {
  const _IntegrationOff({required this.agent});
  final AgentIntegration agent;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return AtlasCard(
      accent: c.warning,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.link_off_rounded, color: c.warning, size: 20),
          const SizedBox(width: Space.sm),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text("${agent.label} integration is off"),
                const SizedBox(height: Space.xs),
                Text(
                  "Its agents won't show here. Turn it on in Atlas on your Mac: Settings → Agents.",
                  style: TextStyle(color: c.muted),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
