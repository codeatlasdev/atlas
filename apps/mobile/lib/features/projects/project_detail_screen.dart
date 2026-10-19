import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/projects/charts.dart';
import 'package:atlas_mobile/features/projects/project_avatar.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

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
  ProjectSnapshot? _snap;
  String? _error;
  Timer? _timer;
  bool _cpuChart = false;

  /// Overview (0) or Metrics (1).
  int _tab = 0;
  final Set<String> _busy = {};

  Multiplexer? get _mux =>
      ref.read(connectionNotifierProvider(widget.device)).value;

  @override
  void initState() {
    super.initState();
    _refresh();
    _timer = Timer.periodic(_poll, (_) => _refresh());
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    final mux = _mux;
    if (mux == null) return;
    final resp = await mux.call('project_detail', {'id': widget.project.id});
    if (!mounted) return;
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
        builder: (_) => _LogsPage(
          mux: _mux,
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
        title: '${a.agent} · ${widget.project.name}',
      ),
    ),
  );

  @override
  Widget build(BuildContext context) {
    // Keep the connection alive while this screen is open, and reload the moment it
    // (re)connects instead of waiting for the next poll.
    ref.listen(connectionNotifierProvider(widget.device), (prev, next) {
      if (prev?.value == null && next.value != null) _refresh();
    });
    final c = AtlasColors.of(context);
    final s = _snap;
    final tint = projectColor(context, widget.project.name);
    return Scaffold(
      body: Stack(
        children: [
          // The project's colour as a faint glow behind its header.
          Positioned(
            top: -180,
            left: -100,
            right: -100,
            height: 440,
            child: IgnorePointer(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  gradient: RadialGradient(
                    colors: [
                      tint.withValues(alpha: .2),
                      tint.withValues(alpha: 0),
                    ],
                  ),
                ),
              ),
            ),
          ),
          if (s == null)
            SafeArea(
              child: Column(
                children: [
                  _TopBar(onLogs: null, title: widget.project.name),
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
                        : const _Skeleton(),
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
                      child: _TopBar(onLogs: () => _openLogs(), title: null),
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
                        _Header(
                          snap: s,
                          onUp: _busy.contains('up')
                              ? null
                              : () => _serviceOp('up'),
                        ),
                        if (s.setupState != null && s.setupState != 'ok')
                          _SetupRow(snap: s),
                        ..._attention(s),
                        const SizedBox(height: Space.sm),
                        _Kpis(snap: s),
                        const SizedBox(height: Space.md),
                        _Tabs(
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
        _Banner(
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
          _Banner(
            color: c.error,
            icon: Icons.error_outline_rounded,
            title: '${svc.name} stopped working',
            action: 'Logs',
            onTap: () => _openLogs(service: svc.name),
          ),
    ];
  }

  /// Every agent of the project, one line each; the ones waiting on you first.
  Widget _agents(ProjectSnapshot s) {
    final list = [
      ...s.agents.where((e) => _needsYou(e.$2)),
      ...s.agents.where((e) => !_needsYou(e.$2)),
    ];
    if (list.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: Space.xl),
        child: Text(
          'No agent running in this project.',
          textAlign: TextAlign.center,
          style: TextStyle(color: AtlasColors.of(context).muted),
        ),
      );
    }
    return Padding(
      padding: const EdgeInsets.only(top: Space.md),
      child: AtlasCard(
        padding: EdgeInsets.zero,
        child: Column(
          children: [
            for (final (pane, a) in list)
              _AgentLine(status: a, onTap: () => _openAgent(pane, a)),
          ],
        ),
      ),
    );
  }

  Widget _overview(ProjectSnapshot s) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      if (s.agents.isNotEmpty) ...[
        const SizedBox(height: Space.md),
        // Agents live in their own tab; here only a one-line summary leading there.
        AtlasCard(
          onTap: () => setState(() => _tab = 1),
          child: Row(
            children: [
              Icon(
                Icons.auto_awesome_rounded,
                size: 18,
                color: AtlasColors.of(context).muted,
              ),
              const SizedBox(width: Space.sm),
              Expanded(
                child: Text(
                  [
                    '${s.agents.length} agent${s.agents.length == 1 ? '' : 's'}',
                    if (s.agents.any((e) => _needsYou(e.$2)))
                      '${s.agents.where((e) => _needsYou(e.$2)).length} need you'
                    else if (s.agents.any((e) => e.$2.state == 'working'))
                      '${s.agents.where((e) => e.$2.state == 'working').length} working',
                  ].join(' · '),
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
              ),
              Icon(Icons.chevron_right, color: AtlasColors.of(context).faint),
            ],
          ),
        ),
      ],
      SectionHeader('Services', count: s.services.length),
      // Two per row; tap a tile for its details and controls.
      LayoutBuilder(
        builder: (context, box) => Wrap(
          spacing: Space.sm,
          children: [
            for (var i = 0; i < s.services.length; i++)
              SizedBox(
                width: (box.maxWidth - Space.sm) / 2,
                child: _ServiceCard(
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
          _ActionTile(action: a, onRun: () => _runAction(a)),
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
          style: const ButtonStyle(visualDensity: VisualDensity.compact),
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
          style: TextStyle(color: AtlasColors.of(context).faint, fontSize: 12),
        ),
      ),
    ],
  );
}

/// Back, and the logs shortcut; the title only while there is no header to carry it.
class _TopBar extends StatelessWidget {
  const _TopBar({required this.onLogs, required this.title});
  final VoidCallback? onLogs;
  final String? title;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: Space.xs),
    child: Row(
      children: [
        const BackButton(),
        if (title != null)
          Expanded(
            child: Text(title!, style: Theme.of(context).textTheme.titleMedium),
          )
        else
          const Spacer(),
        if (onLogs != null)
          TextButton.icon(
            onPressed: onLogs,
            icon: const Icon(Icons.subject_rounded, size: 18),
            label: const Text('Logs'),
          ),
      ],
    ),
  );
}

/// Pill tabs; the selected one slides under the labels.
class _Tabs extends StatelessWidget {
  const _Tabs({
    required this.index,
    required this.labels,
    required this.onChanged,
  });
  final int index;
  final List<String> labels;
  final void Function(int) onChanged;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      height: 40,
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: c.base,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: c.borderSubtle),
      ),
      child: LayoutBuilder(
        builder: (context, box) {
          final w = box.maxWidth / labels.length;
          return Stack(
            children: [
              AnimatedPositioned(
                duration: Motion.slow,
                curve: Motion.ease,
                left: w * index,
                top: 0,
                bottom: 0,
                width: w,
                child: Container(
                  decoration: BoxDecoration(
                    color: c.raised,
                    borderRadius: BorderRadius.circular(9),
                  ),
                ),
              ),
              Row(
                children: [
                  for (var i = 0; i < labels.length; i++)
                    Expanded(
                      child: GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTap: () => onChanged(i),
                        child: Center(
                          child: AnimatedDefaultTextStyle(
                            duration: Motion.normal,
                            style: TextStyle(
                              color: i == index ? c.text : c.muted,
                              fontWeight: i == index
                                  ? FontWeight.w600
                                  : FontWeight.w500,
                              fontSize: 14,
                            ),
                            child: Text(labels[i]),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ],
          );
        },
      ),
    );
  }
}

/// One thing in this project that waits on the user, with its action.
class _Banner extends StatelessWidget {
  const _Banner({
    required this.color,
    required this.icon,
    required this.title,
    required this.action,
    required this.onTap,
  });
  final Color color;
  final IconData icon;
  final String title;
  final String action;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: Space.sm),
    child: GestureDetector(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.fromLTRB(
          Space.md,
          Space.sm,
          Space.sm,
          Space.sm,
        ),
        decoration: BoxDecoration(
          color: color.withValues(alpha: .12),
          borderRadius: BorderRadius.circular(Space.radius),
          border: Border.all(color: color.withValues(alpha: .45)),
        ),
        child: Row(
          children: [
            Icon(icon, color: color, size: 18),
            const SizedBox(width: Space.sm),
            Expanded(
              child: Text(
                title,
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
            ),
            Pill(action, color: color),
          ],
        ),
      ),
    ),
  );
}

/// An atlas.yaml action: what it is, whether it asks first, one tap to run.
class _ActionTile extends StatelessWidget {
  const _ActionTile({required this.action, required this.onRun});
  final ProjectAction action;
  final VoidCallback onRun;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return AtlasCard(
      onTap: onRun,
      padding: const EdgeInsets.fromLTRB(
        Space.md,
        Space.sm + 2,
        Space.md,
        Space.sm + 2,
      ),
      child: Row(
        children: [
          Container(
            width: 32,
            height: 32,
            decoration: BoxDecoration(
              color: c.accent.withValues(alpha: .14),
              borderRadius: BorderRadius.circular(Space.radiusSm),
            ),
            child: Icon(Icons.play_arrow_rounded, color: c.accent, size: 20),
          ),
          const SizedBox(width: Space.md),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  action.label,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                if (action.confirm != null)
                  Text(
                    'Asks before running',
                    style: TextStyle(color: c.faint, fontSize: 12),
                  ),
              ],
            ),
          ),
          Text(
            action.key,
            style: TextStyle(
              color: c.faint,
              fontFamily: monoFamily,
              fontFamilyFallback: monoFallback,
              fontSize: 12,
            ),
          ),
        ],
      ),
    );
  }
}

String _dur(int s) {
  final h = s ~/ 3600, m = (s % 3600) ~/ 60;
  return h > 0
      ? '${h}h ${m}m'
      : m > 0
      ? '${m}m'
      : '${s}s';
}

class _Header extends StatelessWidget {
  const _Header({required this.snap, required this.onUp});
  final ProjectSnapshot snap;
  final VoidCallback? onUp;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final failed = snap.services.where((s) => s.state == 'failed').length;
    final (label, color) = !snap.running
        ? ('Stopped', c.faint)
        : failed > 0
        ? ('$failed failed', c.error)
        : ('Running', c.success);
    final meta = [
      if (snap.running && snap.uptimeS != null) 'up ${_dur(snap.uptimeS!)}',
      if (snap.pid != null) 'pid ${snap.pid}',
    ].join(' · ');
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Hero(
                tag: 'project-${snap.id}',
                child: ProjectAvatar(name: snap.name, size: 56),
              ),
              const SizedBox(width: Space.md),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      snap.name,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.headlineSmall
                          ?.copyWith(
                            fontWeight: FontWeight.w700,
                            letterSpacing: -.5,
                          ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      snap.root.replaceFirst(RegExp(r'^/Users/[^/]+'), '~'),
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 12,
                        color: c.faint,
                        fontFamily: monoFamily,
                        fontFamilyFallback: monoFallback,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: Space.md),
          Row(
            children: [
              Container(
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
                      pulse: snap.running && failed == 0,
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
              if (meta.isNotEmpty) ...[
                const SizedBox(width: Space.sm),
                Text(meta, style: TextStyle(color: c.faint, fontSize: 12)),
              ],
              const Spacer(),
              if (!snap.running)
                FilledButton.icon(
                  onPressed: onUp,
                  icon: const Icon(Icons.play_arrow_rounded),
                  label: const Text('Start'),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

class _SetupRow extends StatelessWidget {
  const _SetupRow({required this.snap});
  final ProjectSnapshot snap;

  @override
  Widget build(BuildContext context) {
    final ok = snap.setupState == 'ok';
    return AtlasCard(
      child: Row(
        children: [
          Icon(
            ok
                ? Icons.check
                : snap.setupState == 'running'
                ? Icons.hourglass_top
                : Icons.error_outline,
            size: 16,
            color: ok
                ? AtlasColors.of(context).success
                : AtlasColors.of(context).error,
          ),
          const SizedBox(width: 8),
          const Text('Setup', style: TextStyle(fontWeight: FontWeight.w600)),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              snap.setupCommand ?? '',
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(
                fontFamily: monoFamily,
                fontFamilyFallback: monoFallback,
                fontSize: 12,
              ),
            ),
          ),
          Text(snap.setupState!, style: const TextStyle(fontSize: 12)),
        ],
      ),
    );
  }
}

class _Kpis extends StatelessWidget {
  const _Kpis({required this.snap});
  final ProjectSnapshot snap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final mem = snap.memMb >= 1024
        ? '${(snap.memMb / 1024).toStringAsFixed(1)} GB'
        : '${snap.memMb.round()} MB';
    final allUp = snap.upCount == snap.services.length;
    // (label, value, colour when it needs attention)
    final items = [
      (
        'running',
        '${snap.upCount}/${snap.services.length}',
        allUp ? null : c.error,
      ),
      ('errors · 5m', '${snap.errors5m}', snap.errors5m > 0 ? c.warning : null),
      ('restarts', '${snap.restarts}', snap.restarts > 0 ? c.warning : null),
      ('memory', mem, null),
      ('cpu', '${snap.cpu.round()}%', null),
    ];
    // One strip, scrollable on narrow phones, like the desktop's KPI row.
    return AtlasCard(
      padding: const EdgeInsets.symmetric(vertical: Space.md),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: Space.md),
        child: IntrinsicHeight(
          child: Row(
            children: [
              for (var i = 0; i < items.length; i++) ...[
                if (i > 0)
                  VerticalDivider(color: c.borderSubtle, width: Space.xl),
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    AnimatedSwitcher(
                      duration: Motion.normal,
                      transitionBuilder: (w, a) =>
                          FadeTransition(opacity: a, child: w),
                      child: Text(
                        items[i].$2,
                        key: ValueKey(items[i].$2),
                        style: TextStyle(
                          fontSize: 20,
                          fontWeight: FontWeight.w700,
                          color: items[i].$3 ?? c.text,
                          fontFeatures: const [FontFeature.tabularFigures()],
                        ),
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      items[i].$1,
                      style: TextStyle(fontSize: 11, color: c.faint),
                    ),
                  ],
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _ServiceCard extends StatelessWidget {
  const _ServiceCard({
    required this.svc,
    required this.mem,
    required this.color,
    required this.busy,
    required this.onOp,
    required this.onLogs,
  });

  final ServiceRow svc;
  final List<double> mem;
  final Color color;
  final bool busy;
  final void Function(String op) onOp;
  final VoidCallback onLogs;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final failed = svc.state == 'failed';
    return AtlasCard(
      padding: const EdgeInsets.fromLTRB(
        Space.md,
        Space.sm + 2,
        Space.md,
        Space.sm + 2,
      ),
      accent: failed ? c.error : null,
      onTap: () => _ServiceSheet.show(context, this),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              StatusDot(
                color: c.serviceState(svc.state),
                pulse: svc.state == 'starting',
                size: 6,
              ),
              Expanded(
                child: Text(
                  svc.name,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontWeight: FontWeight.w600,
                    fontSize: 15,
                  ),
                ),
              ),
              if (busy)
                const SizedBox.square(
                  dimension: 14,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
            ],
          ),
          const SizedBox(height: 2),
          Text(
            svc.note ??
                (svc.port != null && svc.up
                    ? '${svc.state} · :${svc.port}'
                    : svc.state),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(fontSize: 12, color: failed ? c.error : c.faint),
          ),
          const SizedBox(height: Space.sm),
          SizedBox(
            height: 22,
            child: svc.up
                ? Sparkline(values: mem, color: color)
                : Align(
                    alignment: Alignment.centerLeft,
                    child: Container(height: 1, color: c.borderSubtle),
                  ),
          ),
          const SizedBox(height: 4),
          Text(
            svc.up ? '${svc.memMb.round()} MB · ${svc.cpu.round()}%' : '—',
            style: TextStyle(
              fontSize: 11,
              color: c.faint,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
        ],
      ),
    );
  }
}

/// A service's details and controls, from its tile: state, port, last log line,
/// dependencies, and restart / stop / start / logs.
class _ServiceSheet extends StatelessWidget {
  const _ServiceSheet(this.card);
  final _ServiceCard card;

  static Future<void> show(BuildContext context, _ServiceCard card) =>
      showModalBottomSheet<void>(
        context: context,
        builder: (_) => _ServiceSheet(card),
      );

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final svc = card.svc;
    final failed = svc.state == 'failed';
    void run(String op) {
      Navigator.pop(context);
      card.onOp(op);
    }

    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, Space.lg),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                StatusDot(color: c.serviceState(svc.state), size: 8),
                Text(svc.name, style: Theme.of(context).textTheme.titleLarge),
                const SizedBox(width: Space.sm),
                Flexible(
                  child: Text(
                    svc.note ?? svc.state,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(color: failed ? c.error : c.muted),
                  ),
                ),
                const Spacer(),
                if (svc.port != null)
                  Pill(
                    ':${svc.port}',
                    color: c.accent,
                    mono: true,
                    onTap: () {
                      Clipboard.setData(
                        ClipboardData(text: 'http://localhost:${svc.port}'),
                      );
                      ScaffoldMessenger.of(context).showSnackBar(
                        SnackBar(content: Text('localhost:${svc.port} copied')),
                      );
                    },
                  ),
              ],
            ),
            if (svc.up) ...[
              const SizedBox(height: Space.md),
              SizedBox(
                height: 40,
                child: Sparkline(values: card.mem, color: card.color),
              ),
              const SizedBox(height: 4),
              Text(
                '${svc.memMb.round()} MB · ${svc.cpu.round()}% CPU'
                '${svc.restarts > 0 ? ' · ${svc.restarts} restarts' : ''}',
                style: TextStyle(color: c.faint, fontSize: 12),
              ),
            ],
            if (svc.lastLine.isNotEmpty) ...[
              const SizedBox(height: Space.md),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(Space.sm + 2),
                decoration: BoxDecoration(
                  color: c.ground,
                  borderRadius: BorderRadius.circular(Space.radiusSm),
                ),
                child: Text(
                  svc.lastLine,
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontFamily: monoFamily,
                    fontFamilyFallback: monoFallback,
                    fontSize: 12,
                    color: failed ? c.error : c.muted,
                  ),
                ),
              ),
            ],
            if (svc.dependsOn.isNotEmpty) ...[
              const SizedBox(height: Space.sm),
              Text(
                'Needs ${svc.dependsOn.join(', ')}',
                style: TextStyle(color: c.faint, fontSize: 12),
              ),
            ],
            const SizedBox(height: Space.lg),
            Row(
              children: [
                if (svc.up) ...[
                  Expanded(
                    child: FilledButton.icon(
                      onPressed: () => run('restart'),
                      icon: const Icon(Icons.refresh_rounded, size: 18),
                      label: const Text('Restart'),
                    ),
                  ),
                  const SizedBox(width: Space.sm),
                  Expanded(
                    child: OutlinedButton.icon(
                      onPressed: () => run('stop'),
                      icon: const Icon(Icons.stop_rounded, size: 18),
                      label: const Text('Stop'),
                    ),
                  ),
                ] else
                  Expanded(
                    child: FilledButton.icon(
                      onPressed: () => run('start'),
                      icon: const Icon(Icons.play_arrow_rounded, size: 18),
                      label: const Text('Start'),
                    ),
                  ),
                const SizedBox(width: Space.sm),
                Expanded(
                  child: OutlinedButton.icon(
                    onPressed: () {
                      Navigator.pop(context);
                      card.onLogs();
                    },
                    icon: const Icon(Icons.subject_rounded, size: 18),
                    label: const Text('Logs'),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// Logs, full screen: filter by service, search, and follow live. Lines are split into
/// service tag, time and text; stderr and error lines stand out.
class _LogsPage extends StatefulWidget {
  const _LogsPage({
    required this.mux,
    required this.projectId,
    required this.services,
    this.service,
  });
  final Multiplexer? mux;
  final String projectId;
  final List<String> services;
  final String? service;

  @override
  State<_LogsPage> createState() => _LogsPageState();
}

/// `[api:12] 12:01:02 [stderr] text` → (service, time, stderr, text).
final _logLine = RegExp(
  r'^\[([^:\]]+)(?::\d+)?\]\s+(\d\d:\d\d:\d\d)\s+\[(stdout|stderr)\]\s?(.*)$',
);

class _LogsPageState extends State<_LogsPage> {
  final _search = TextEditingController();
  late String? _service = widget.service;
  List<String> _lines = const [];
  String? _error;
  bool _loading = true;
  bool _live = true;
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _load();
    _timer = Timer.periodic(const Duration(seconds: 3), (_) {
      if (_live && _search.text.trim().isEmpty) _load(quiet: true);
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    _search.dispose();
    super.dispose();
  }

  Future<void> _load({bool quiet = false}) async {
    final mux = widget.mux;
    if (mux == null) return;
    if (!quiet) setState(() => _loading = true);
    final q = _search.text.trim();
    final resp = await mux.call('project_logs', {
      'id': widget.projectId,
      'service': ?_service,
      'lines': 300,
      if (q.isNotEmpty) 'query': q,
    });
    if (!mounted) return;
    setState(() {
      _loading = false;
      if (resp['ok'] == true) {
        _error = null;
        _lines = (resp['data']?['text'] as String? ?? '')
            .split('\n')
            .where((l) => l.trim().isNotEmpty)
            .toList();
      } else {
        _error = resp['error'] as String? ?? 'error';
      }
    });
  }

  void _pick(String? service) {
    setState(() => _service = service);
    _load();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Scaffold(
      appBar: AppBar(
        title: Text(_service == null ? 'Logs' : 'Logs · $_service'),
        actions: [
          TextButton.icon(
            onPressed: () => setState(() => _live = !_live),
            icon: StatusDot(
              color: _live ? c.success : c.faint,
              pulse: _live,
              size: 6,
            ),
            label: Text(
              _live ? 'Live' : 'Paused',
              style: TextStyle(color: _live ? c.success : c.muted),
            ),
          ),
        ],
      ),
      body: Column(
        children: [
          SizedBox(
            height: 44,
            child: ListView(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: Space.md),
              children: [
                for (final s in [null, ...widget.services])
                  Padding(
                    padding: const EdgeInsets.only(right: 6, top: 6, bottom: 6),
                    child: ChoiceChip(
                      label: Text(s ?? 'All'),
                      selected: _service == s,
                      showCheckmark: false,
                      onSelected: (_) => _pick(s),
                      side: BorderSide(color: c.borderSubtle),
                      selectedColor: c.raised,
                      backgroundColor: c.base,
                      labelStyle: TextStyle(
                        color: _service == s ? c.text : c.muted,
                        fontSize: 13,
                      ),
                      visualDensity: VisualDensity.compact,
                    ),
                  ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(
              Space.md,
              Space.xs,
              Space.md,
              Space.sm,
            ),
            child: TextField(
              controller: _search,
              textInputAction: TextInputAction.search,
              decoration: InputDecoration(
                hintText: 'Search (text or /regex/)',
                prefixIcon: Icon(Icons.search_rounded, color: c.faint),
              ),
              onSubmitted: (_) => _load(),
            ),
          ),
          Expanded(
            child: _loading && _lines.isEmpty
                ? const Center(child: CircularProgressIndicator())
                : _error != null
                ? EmptyState(title: 'Could not read logs', message: _error!)
                : _lines.isEmpty
                ? EmptyState(
                    icon: Icon(Icons.subject_rounded, size: 48, color: c.faint),
                    title: 'No log lines yet',
                    message: 'Lines show up here as the services write them.',
                  )
                : ListView.builder(
                    reverse: true,
                    padding: const EdgeInsets.fromLTRB(
                      Space.md,
                      0,
                      Space.md,
                      Space.lg,
                    ),
                    itemCount: _lines.length,
                    itemBuilder: (_, i) => _LogRow(
                      line: _lines[_lines.length - 1 - i],
                      showService: _service == null,
                      services: widget.services,
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

class _LogRow extends StatelessWidget {
  const _LogRow({
    required this.line,
    required this.showService,
    required this.services,
  });
  final String line;
  final bool showService;
  final List<String> services;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    const mono = TextStyle(
      fontFamily: monoFamily,
      fontFamilyFallback: monoFallback,
      fontSize: 12,
      height: 1.4,
    );
    final m = _logLine.firstMatch(line);
    if (m == null) {
      return SelectableText(line, style: mono.copyWith(color: c.muted));
    }
    final service = m[1]!, time = m[2]!, err = m[3] == 'stderr', text = m[4]!;
    final bad =
        err ||
        RegExp(
          r'\b(error|panic|fatal|exception)\b',
          caseSensitive: false,
        ).hasMatch(text);
    final i = services.indexOf(service);
    final tag = c.series[(i < 0 ? service.length : i) % c.series.length];
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 1),
      child: SelectableText.rich(
        TextSpan(
          style: mono,
          children: [
            TextSpan(
              text: '$time ',
              style: TextStyle(color: c.faint),
            ),
            if (showService)
              TextSpan(
                text: '$service ',
                style: TextStyle(color: tag, fontWeight: FontWeight.w600),
              ),
            TextSpan(
              text: text,
              style: TextStyle(color: bad ? c.error : c.text),
            ),
          ],
        ),
      ),
    );
  }
}

bool _needsYou(AgentStatus a) => a.state == 'blocked' || a.state == 'waiting';

/// One agent as a line: state dot, name, state, last message.
class _AgentLine extends StatelessWidget {
  const _AgentLine({required this.status, required this.onTap});
  final AgentStatus status;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final k = c.agentState(status.state);
    final line = status.state == 'working'
        ? (status.prompt ?? '')
        : (status.lastMessage ?? status.prompt ?? '');
    return InkWell(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Space.md,
          vertical: Space.sm + 2,
        ),
        child: Row(
          children: [
            StatusDot(color: k, pulse: status.state == 'working'),
            const SizedBox(width: Space.sm),
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
                      const SizedBox(width: Space.sm),
                      Text(
                        agentStateLabel(status.state),
                        style: TextStyle(
                          color: k,
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ],
                  ),
                  if (line.isNotEmpty)
                    Text(
                      line.replaceAll('\n', ' '),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(fontSize: 12, color: c.muted),
                    ),
                ],
              ),
            ),
            Icon(Icons.chevron_right, color: c.faint),
          ],
        ),
      ),
    );
  }
}

/// The panel's shape, shimmering, while the first `project_detail` is on its way.
class _Skeleton extends StatefulWidget {
  const _Skeleton();

  @override
  State<_Skeleton> createState() => _SkeletonState();
}

class _SkeletonState extends State<_Skeleton>
    with SingleTickerProviderStateMixin {
  late final AnimationController _a = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1200),
  )..repeat(reverse: true);

  @override
  void dispose() {
    _a.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    Widget bar(double h, {double? w}) => Container(
      height: h,
      width: w,
      margin: const EdgeInsets.only(bottom: Space.sm),
      decoration: BoxDecoration(
        color: c.base,
        borderRadius: BorderRadius.circular(Space.radius),
      ),
    );
    return FadeTransition(
      opacity: Tween(begin: .45, end: 1.0).animate(_a),
      child: ListView(
        physics: const NeverScrollableScrollPhysics(),
        padding: const EdgeInsets.all(Space.lg),
        children: [
          Row(
            children: [
              bar(44, w: 44),
              const SizedBox(width: Space.md),
              Expanded(child: bar(14)),
            ],
          ),
          const SizedBox(height: Space.sm),
          bar(64),
          const SizedBox(height: Space.lg),
          bar(12, w: 90),
          for (var i = 0; i < 4; i++) bar(86),
        ],
      ),
    );
  }
}
