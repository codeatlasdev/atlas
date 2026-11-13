import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/features/home/onboarding.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';
import 'package:atlas_mobile/features/projects/project_avatar.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';

/// Strips the trailing `.local` mDNS suffix and converts hyphens to spaces
/// for friendlier display: "MacBook-Pro-de-Luan.local" → "MacBook Pro de Luan".
String friendlyDeviceName(String raw) {
  final stripped = raw.endsWith('.local')
      ? raw.substring(0, raw.length - 6)
      : raw;
  return stripped.replaceAll('-', ' ');
}

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

  /// What the home shows: onboarding, the demo computer, or a paired one's dashboard.
  ({bool onboard, MacDevice? device, List<MacDevice> others}) _target(
    List<MacDevice> stored,
  ) {
    if (ref.watch(demoModeProvider)) {
      return (onboard: false, device: demoDevice, others: const []);
    }
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

  void _openProject(ProjectInfo p) =>
      context.push('/project', extra: {'device': widget.device, 'project': p});

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final conn = ref.watch(connectionNotifierProvider(widget.device));
    final connected = conn.value != null;
    // Always watch so the last loaded value survives reconnects.
    final projects = ref.watch(projectListProvider(widget.device));
    // Live agent states override the list's snapshot as soon as they are pushed.
    final live = connected
        ? ref.watch(agentStatusMapNotifierProvider(widget.device)).value ??
              const {}
        : const <String, AgentStatus>{};

    final list = projects.value ?? const <ProjectInfo>[];
    final agentsOf = <String, List<(String, AgentStatus)>>{};
    for (final p in list) {
      for (final (pane, s0) in p.agents) {
        (agentsOf[p.id] ??= []).add((pane, live[pane] ?? s0));
      }
    }
    final attention = attentionFor(
      widget.device,
      projects: list,
      live: live,
      bots: const [],
    );
    // Projects that need you, then running, then stopped.
    final flagged = {
      for (final a in attention)
        if (a
            case AgentAttention(:final project) ||
                ServiceAttention(:final project))
          project.id,
    };
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
                    status: !connected || projects.value == null
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
                  ),
                ),
              ),
              // ── Content slivers ──────────────────────────────────────────
              if (!connected)
                SliverFillRemaining(
                  hasScrollBody: false,
                  child: _Offline(
                    device: widget.device,
                    connecting: conn.isLoading,
                  ),
                )
              else ...[
                // Attention cards + integrations + section header
                SliverPadding(
                  padding: const EdgeInsets.fromLTRB(
                    Space.lg,
                    Space.sm,
                    Space.lg,
                    0,
                  ),
                  sliver: SliverList.list(
                    children: [
                      for (final a in missing) _IntegrationOff(agent: a),
                      if (missing.isNotEmpty) const SizedBox(height: Space.md),
                      SectionHeader('Projects', count: list.length),
                    ],
                  ),
                ),
                // Empty state centred in remaining space, or the project list
                if (list.isEmpty)
                  const SliverFillRemaining(
                    hasScrollBody: false,
                    child: Padding(
                      padding: EdgeInsets.only(bottom: kFloatingBarScrollPad),
                      child: _NoProjectsYet(),
                    ),
                  )
                else
                  SliverPadding(
                    padding: const EdgeInsets.fromLTRB(
                      0,
                      Space.xs,
                      0,
                      Space.xxl + kFloatingBarScrollPad,
                    ),
                    sliver: SliverList.list(
                      children: [
                        for (var i = 0; i < sorted.length; i++)
                          EnterFade(
                            index: i,
                            child: _ProjectTile(
                              project: sorted[i],
                              agents: agentsOf[sorted[i].id] ?? const [],
                              flagged: flagged.contains(sorted[i].id),
                              onOpen: () => _openProject(sorted[i]),
                              last: i == sorted.length - 1,
                            ),
                          ),
                      ],
                    ),
                  ),
              ],
            ],
          ),
        ),
      ],
    );
  }
}

/// Logo, computer name (tap to switch), and one status pill that says it all.
class _Header extends StatelessWidget {
  const _Header({
    required this.device,
    required this.conn,
    required this.others,
    required this.onSwitch,
    required this.status,
  });

  final MacDevice device;
  final AsyncValue<Object?> conn;
  final List<MacDevice> others;
  final void Function(MacDevice) onSwitch;

  /// (label, colour) once connected and loaded; null shows the connection state.
  final (String, Color)? status;

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
            friendlyDeviceName(device.name),
            overflow: TextOverflow.ellipsis,
            style: Theme.of(context).textTheme.titleMedium
                ?.copyWith(fontSize: 20, fontWeight: FontWeight.w600),
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
        ],
      ),
    );
  }
}

/// A project, Telegram-row style like the crew's logbook: avatar with a running dot, the name
/// and its state on top, under it what its agent is doing (tap: that agent) or its services.
class _ProjectTile extends StatelessWidget {
  const _ProjectTile({
    required this.project,
    required this.agents,
    required this.flagged,
    required this.onOpen,
    this.last = false,
  });

  final ProjectInfo project;
  final List<(String, AgentStatus)> agents;
  final bool flagged;
  final VoidCallback onOpen;

  /// The last row draws no divider under it.
  final bool last;

  /// The agent worth showing: one that needs you, else one working, else the latest.
  (String, AgentStatus)? get _lead {
    if (agents.isEmpty) return null;
    int rank(AgentStatus s) => switch (s.state) {
      'blocked' || 'waiting' => 0,
      'working' => 1,
      _ => 2,
    };
    final sorted = [...agents]
      ..sort((a, b) {
        final r = rank(a.$2).compareTo(rank(b.$2));
        return r != 0 ? r : b.$2.stateStartedAt.compareTo(a.$2.stateStartedAt);
      });
    return sorted.first;
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final up = project.services.where((s) => s.state == 'running').length;
    final failed = project.services.where((s) => s.state == 'failed').length;
    final lead = _lead;
    final (stateText, stateColor) = !project.running
        ? ('stopped', c.faint)
        : failed > 0
        ? ('$failed down', c.error)
        : project.services.isEmpty
        ? ('running', c.success)
        : ('$up/${project.services.length} up', c.success);
    return InkWell(
      onTap: onOpen,
      child: Padding(
        padding: const EdgeInsets.only(left: Space.lg),
        child: Row(
          children: [
            // Flies into the project screen's header; the dot says if it runs.
            Stack(
              clipBehavior: Clip.none,
              children: [
                Hero(
                  tag: 'project-${project.id}',
                  child: ProjectAvatar(name: project.name, size: 50),
                ),
                if (project.running)
                  Positioned(
                    right: -2,
                    bottom: -2,
                    child: Container(
                      width: 15,
                      height: 15,
                      decoration: BoxDecoration(
                        color: failed > 0 ? c.error : c.success,
                        shape: BoxShape.circle,
                        border: Border.all(color: c.ground, width: 2.5),
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Container(
                padding: const EdgeInsets.fromLTRB(0, 12, Space.lg, 12),
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
                        Expanded(
                          child: Text(
                            project.name,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 16,
                              fontWeight: FontWeight.w600,
                              color: c.text,
                            ),
                          ),
                        ),
                        Text(
                          stateText,
                          style: TextStyle(fontSize: 13, color: stateColor),
                        ),
                      ],
                    ),
                    const SizedBox(height: 3),
                    if (lead == null)
                      Text(
                        project.running ? 'No agent here' : 'Tap to open',
                        style: TextStyle(fontSize: 14.5, color: c.faint),
                      )
                    else
                      _AgentLine(status: lead.$2, more: agents.length - 1),
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

/// What a project's agent is doing, in one line: its logo, its words, its state.
class _AgentLine extends StatelessWidget {
  const _AgentLine({required this.status, required this.more});
  final AgentStatus status;
  final int more;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final s = status;
    final busy = s.state == 'working';
    final asks = s.state == 'blocked' || s.state == 'waiting';
    final text = ((busy ? s.prompt : s.lastMessage ?? s.prompt) ?? '')
        .replaceAll('\n', ' ');
    // Not tappable on its own: the whole row opens the overview, where the agents are.
    return Row(
      children: [
        AgentLogo(s.agent, size: 15),
        const SizedBox(width: 6),
        Expanded(
          child: Text(
            busy
                ? 'working… $text'
                : asks
                ? 'needs you · $text'
                : text,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 14.5,
              color: busy
                  ? c.accent
                  : asks
                  ? c.warning
                  : c.muted,
            ),
          ),
        ),
        if (more > 0) ...[
          const SizedBox(width: 6),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
            decoration: BoxDecoration(
              color: c.raised,
              borderRadius: BorderRadius.circular(8),
            ),
            child: Text(
              '+$more',
              style: TextStyle(fontSize: 11.5, color: c.muted),
            ),
          ),
        ],
      ],
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
                  "Its agents won't show here. Accept Atlas's offer on your Mac, or run `atlas setup ${agent.setup} --apply` there.",
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

class _NoProjectsYet extends StatelessWidget {
  const _NoProjectsYet();

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return EmptyState(
      icon: Icon(Icons.folder_outlined, size: 56, color: c.faint),
      title: 'No projects yet',
      message: 'Open a folder in Atlas on your Mac and it shows up here.',
      action: Container(
        padding: const EdgeInsets.symmetric(
          horizontal: Space.lg,
          vertical: Space.sm,
        ),
        decoration: BoxDecoration(
          color: c.raised,
          borderRadius: BorderRadius.circular(Space.radius),
        ),
        child: Text(
          'Atlas Desktop: File → Open Folder\nor run `atlas .` in a project',
          style: TextStyle(
            color: c.muted,
            fontSize: 12,
            fontFamily: monoFamily,
            fontFamilyFallback: monoFallback,
          ),
          textAlign: TextAlign.center,
        ),
      ),
    );
  }
}
