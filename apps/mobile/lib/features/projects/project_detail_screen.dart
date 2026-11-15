import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/latest_only.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/protocol/pty_open.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/agent_launcher.dart';
import 'package:atlas_mobile/features/git/changes_tile.dart';
import 'package:atlas_mobile/features/projects/charts.dart';
import 'package:atlas_mobile/features/projects/detail/action_tile.dart';
import 'package:atlas_mobile/features/projects/detail/agents_tab.dart';
import 'package:atlas_mobile/features/projects/detail/banner.dart';
import 'package:atlas_mobile/features/projects/detail/glow.dart';
import 'package:atlas_mobile/features/projects/detail/header.dart';
import 'package:atlas_mobile/features/projects/detail/kpis.dart';
import 'package:atlas_mobile/features/projects/detail/service_card.dart';
import 'package:atlas_mobile/features/projects/detail/setup_row.dart';
import 'package:atlas_mobile/features/projects/detail/skeleton.dart';
import 'package:atlas_mobile/features/projects/detail/tabs.dart';
import 'package:atlas_mobile/features/projects/detail/top_bar.dart';
import 'package:atlas_mobile/features/projects/logs_page.dart';
import 'package:atlas_mobile/features/projects/project_avatar.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';
import 'package:atlas_mobile/features/terminal/new_terminal.dart';

/// A project's panel, like the desktop's: header, KPIs, service cards, resources and
/// health over time. Polls `project_detail` every [_poll] while open.
class ProjectDetailScreen extends ConsumerStatefulWidget {
  const ProjectDetailScreen({
    super.key,
    required this.device,
    required this.project,
  });

  final MacDevice device;
  final ProjectInfo project;

  @override
  ConsumerState<ProjectDetailScreen> createState() =>
      _ProjectDetailScreenState();
}

const _poll = Duration(seconds: 3);

class _ProjectDetailScreenState extends ConsumerState<ProjectDetailScreen> {
  final _history = ProjectHistory();
  final _latest = LatestOnly();
  ProjectSnapshot? _snap;
  String? _error;
  Timer? _timer;
  bool _cpuChart = false;

  /// Overview (0) or Metrics (1).
  int _tab = 0;
  final Set<String> _busy = {};

  Multiplexer? get _mux => liveMux(ref, widget.device);

  @override
  void initState() {
    super.initState();
    _refresh();
    _timer = Timer.periodic(_poll, (_) => _refresh(poll: true));
  }

  @override
  void dispose() {
    _latest.close();
    _timer?.cancel();
    super.dispose();
  }

  /// Loads `project_detail`. A poll tick ([poll]) skips while the app is in the background, while the
  /// computer is not connected, and while its previous request is still unanswered, so a slow link
  /// carries one request, not a queue. Any other call (opening, pull to refresh, after an action)
  /// always asks. Only the newest question's reply is applied: a late older one must not roll the
  /// screen back or add a sample to the history.
  Future<void> _refresh({bool poll = false}) async {
    final mux = _mux;
    if (mux == null) return;
    if (poll && !appInForeground()) return;
    final ticket = poll ? _latest.tryBegin() : _latest.begin();
    if (ticket == null) return;
    try {
      final resp = await mux.call('project_detail', {'id': widget.project.id});
      if (!mounted || !ticket.isCurrent) return;
      setState(() {
        if (resp['ok'] == true) {
          _snap = ProjectSnapshot.fromDetail(
            (resp['data'] as Map).cast<String, dynamic>(),
          );
          _history.add(_snap!);
          _error = null;
        } else {
          _error = resp['error'] as String? ?? 'error';
        }
      });
    } finally {
      ticket.done();
    }
  }

  void _toast(String text) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
  }

  Future<void> _serviceOp(String op, {String? service}) async {
    final mux = _mux;
    if (mux == null) return;
    final tag = service ?? op;
    setState(() => _busy.add(tag));
    final resp = await mux.call('project_service', {
      'id': widget.project.id,
      'service': ?service,
      'op': op,
    });
    if (!mounted) return;
    setState(() => _busy.remove(tag));
    if (resp['ok'] != true) _toast(resp['error'] as String? ?? 'Failed');
    _refresh();
  }

  Future<void> _runAction(ProjectAction a) async {
    final mux = _mux;
    if (mux == null) return;
    if (a.confirm != null) {
      final go = await showDialog<bool>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: Text(a.label),
          content: Text(a.confirm!),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('Run'),
            ),
          ],
        ),
      );
      if (go != true) return;
    }
    final resp = await mux.call('project_action', {
      'id': widget.project.id,
      'key': a.key,
      'confirmed': a.confirm != null,
    });
    final text = resp['ok'] == true
        ? (resp['data']?['text'] as String?)
        : (resp['error'] as String?);
    _toast(text == null || text.isEmpty ? '${a.label}: started' : text);
  }

  void _openLogs({String? service}) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => LogsPage(
          device: widget.device,
          projectId: widget.project.id,
          services: [
            for (final s in _snap?.services ?? const <ServiceRow>[]) s.name,
          ],
          service: service,
        ),
      ),
    );
  }

  void _openAgent(String pane, AgentStatus a) => Navigator.of(context).push(
    MaterialPageRoute<void>(
      builder: (_) => AgentChatScreen(
        device: widget.device,
        paneKey: pane,
        initial: a,
        title: agentTitle(a),
      ),
    ),
  );

  /// Pick an agent and a first prompt, launch it here, then open its chat.
  Future<void> _newAgent() async {
    final launched = await showAgentLauncher(
      context,
      device: widget.device,
      projectId: widget.project.id,
    );
    if (launched == null || !mounted) return;
    _openAgent(
      launched.pane,
      AgentStatus(
        state: 'working',
        agent: launched.agent,
        updatedAt: DateTime.now().millisecondsSinceEpoch,
        stateStartedAt: DateTime.now().millisecondsSinceEpoch,
      ),
    );
    _refresh();
  }

  @override
  Widget build(BuildContext context) {
    // Keep the connection alive while this screen is open, and reload the moment it
    // (re)connects instead of waiting for the next poll.
    ref.listen(connectionNotifierProvider(widget.device), (prev, next) {
      if (prev?.value == null && next.value != null) _refresh();
    });
    final c = AtlasColors.of(context);
    final s = _snap;
    final reconnecting = ref
        .watch(connectionNotifierProvider(widget.device))
        .isLoading;
    final tint = projectColor(context, widget.project.name);
    return Scaffold(
      body: Stack(
        children: [
          ProjectGlow(tint: tint),
          if (s == null)
            SafeArea(
              child: Column(
                children: [
                  // Same top bar as the loaded page: the header below names it.
                  ProjectTopBar(
                    reconnecting: reconnecting,
                    onLogs: null,
                    title: _error != null ? widget.project.name : null,
                  ),
                  Expanded(
                    child: _error != null
                        ? EmptyState(
                            title: 'Could not load ${widget.project.name}',
                            message: _error!,
                            action: OutlinedButton(
                              onPressed: _refresh,
                              child: const Text('Try again'),
                            ),
                          )
                        : ProjectSkeleton(project: widget.project),
                  ),
                ],
              ),
            )
          else
            RefreshIndicator(
              color: c.accent,
              onRefresh: _refresh,
              child: CustomScrollView(
                slivers: [
                  SliverSafeArea(
                    bottom: false,
                    sliver: SliverToBoxAdapter(
                      child: ProjectTopBar(
                        onLogs: () => _openLogs(),
                        title: null,
                        reconnecting: reconnecting,
                      ),
                    ),
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
                        ProjectHeader(
                          snap: s,
                          onUp: _busy.contains('up')
                              ? null
                              : () => _serviceOp('up'),
                        ),
                        if (s.setupState != null && s.setupState != 'ok')
                          ProjectSetupRow(snap: s),
                        ..._attention(s),
                        const SizedBox(height: Space.sm),
                        ProjectKpis(snap: s),
                        const SizedBox(height: Space.md),
                        ProjectTabs(
                          index: _tab,
                          labels: [
                            'Overview',
                            s.agents.isEmpty
                                ? 'Agents'
                                : 'Agents ${s.agents.length}',
                            'Metrics',
                          ],
                          onChanged: (i) => setState(() => _tab = i),
                        ),
                        AnimatedSwitcher(
                          duration: Motion.normal,
                          switchInCurve: Motion.ease,
                          transitionBuilder: (w, a) => FadeTransition(
                            opacity: a,
                            child: SlideTransition(
                              position: Tween(
                                begin: const Offset(0, .02),
                                end: Offset.zero,
                              ).animate(a),
                              child: w,
                            ),
                          ),
                          child: KeyedSubtree(
                            key: ValueKey(_tab),
                            child: switch (_tab) {
                              0 => _overview(s),
                              1 => _agents(s),
                              _ => _metrics(),
                            },
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }

  /// What in this project waits on the user: agent prompts, then failed services.
  List<Widget> _attention(ProjectSnapshot s) {
    final c = AtlasColors.of(context);
    return [
      for (final (pane, a) in s.agents.where(
        (x) => x.$2.state == 'blocked' || x.$2.state == 'waiting',
      ))
        ProjectBanner(
          color: c.warning,
          icon: Icons.front_hand_rounded,
          title: a.state == 'blocked'
              ? '${a.agent} is waiting for your approval'
              : '${a.agent} asked you something',
          action: a.state == 'blocked' ? 'Review' : 'Answer',
          onTap: () => _openAgent(pane, a),
        ),
      if (s.running)
        for (final svc in s.services.where((x) => x.state == 'failed'))
          ProjectBanner(
            color: c.error,
            icon: Icons.error_outline_rounded,
            title: '${svc.name} stopped working',
            action: 'Logs',
            onTap: () => _openLogs(service: svc.name),
          ),
    ];
  }

  /// Every agent of the project, one line each; the ones waiting on you first.
  Widget _agents(ProjectSnapshot s) => ProjectAgentsTab(
    device: widget.device,
    projectId: widget.project.id,
    agents: s.agents,
    onNewAgent: _newAgent,
    onNewTerminal: canOpenTerminal(widget.device)
        ? () =>
              newTerminalInProject(context, ref, widget.device, widget.project)
        : null,
    onOpenAgent: _openAgent,
  );

  Widget _overview(ProjectSnapshot s) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      ChangesTile(device: widget.device, project: widget.project),
      SectionHeader('Services', count: s.services.length),
      // Two per row; tap a tile for its details and controls.
      LayoutBuilder(
        builder: (context, box) => Wrap(
          spacing: Space.sm,
          children: [
            for (var i = 0; i < s.services.length; i++)
              SizedBox(
                width: (box.maxWidth - Space.sm) / 2,
                child: ServiceCard(
                  svc: s.services[i],
                  mem: _history.mem[s.services[i].name] ?? const [],
                  color: AtlasColors.of(context)
                      .series[i % AtlasColors.of(context).series.length],
                  busy: _busy.contains(s.services[i].name),
                  onOp: (op) => _serviceOp(op, service: s.services[i].name),
                  onLogs: () => _openLogs(service: s.services[i].name),
                ),
              ),
          ],
        ),
      ),
      if (s.actions.isNotEmpty) ...[
        SectionHeader('Actions', count: s.actions.length),
        for (final a in s.actions)
          ProjectActionTile(action: a, onRun: () => _runAction(a)),
      ],
    ],
  );

  Widget _metrics() => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      SectionHeader(
        'Resources',
        trailing: SegmentedButton<bool>(
          segments: const [
            ButtonSegment(value: false, label: Text('Memory')),
            ButtonSegment(value: true, label: Text('CPU')),
          ],
          selected: {_cpuChart},
          showSelectedIcon: false,
          onSelectionChanged: (v) => setState(() => _cpuChart = v.first),
        ),
      ),
      AtlasCard(
        child: LinesChart(
          series: _cpuChart ? _history.cpu : _history.mem,
          unit: _cpuChart ? '%' : 'MB',
        ),
      ),
      const SectionHeader('Health over time'),
      AtlasCard(child: HealthTimeline(states: _history.states)),
      Padding(
        padding: const EdgeInsets.only(top: Space.xs, left: 2),
        child: Text(
          'Collected while this screen is open, every ${_poll.inSeconds} s.',
          style: TextStyle(color: AtlasColors.of(context).muted, fontSize: 12),
        ),
      ),
    ],
  );
}
