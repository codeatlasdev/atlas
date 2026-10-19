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
          service: service,
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    // Keep the connection alive while this screen is open, and reload the moment it
    // (re)connects instead of waiting for the next poll.
    ref.listen(connectionNotifierProvider(widget.device), (prev, next) {
      if (prev?.value == null && next.value != null) _refresh();
    });
    final s = _snap;
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.project.name),
        actions: [
          IconButton(
            icon: const Icon(Icons.article_outlined),
            tooltip: 'Logs',
            onPressed: () => _openLogs(),
          ),
        ],
      ),
      body: s == null
          ? (_error != null
                ? EmptyState(
                    title: 'Could not load ${widget.project.name}',
                    message: _error!,
                    action: OutlinedButton(
                      onPressed: _refresh,
                      child: const Text('Try again'),
                    ),
                  )
                : const _Skeleton())
          : RefreshIndicator(
              onRefresh: _refresh,
              child: ListView(
                padding: const EdgeInsets.fromLTRB(
                  Space.lg,
                  Space.xs,
                  Space.lg,
                  Space.xxl,
                ),
                children: [
                  _Header(
                    snap: s,
                    onUp: _busy.contains('up') ? null : () => _serviceOp('up'),
                  ),
                  if (s.setupState != null && s.setupState != 'ok')
                    _SetupRow(snap: s),
                  const SizedBox(height: 8),
                  _Kpis(snap: s),
                  const SizedBox(height: 16),
                  SectionHeader('Services', count: s.services.length),
                  for (var i = 0; i < s.services.length; i++)
                    _ServiceCard(
                      svc: s.services[i],
                      mem: _history.mem[s.services[i].name] ?? const [],
                      color: AtlasColors.of(context)
                          .series[i % AtlasColors.of(context).series.length],
                      busy: _busy.contains(s.services[i].name),
                      onOp: (op) => _serviceOp(op, service: s.services[i].name),
                      onLogs: () => _openLogs(service: s.services[i].name),
                    ),
                  if (s.agents.isNotEmpty) ...[
                    const SizedBox(height: 8),
                    SectionHeader('Agents', count: s.agents.length),
                    for (final (pane, a) in s.agents)
                      _AgentRow(
                        status: a,
                        onTap: () => Navigator.of(context).push(
                          MaterialPageRoute<void>(
                            builder: (_) => AgentChatScreen(
                              device: widget.device,
                              paneKey: pane,
                              initial: a,
                              title: '${a.agent} · ${widget.project.name}',
                            ),
                          ),
                        ),
                      ),
                  ],
                  if (s.actions.isNotEmpty) ...[
                    const SizedBox(height: 8),
                    const SectionHeader('Actions'),
                    Wrap(
                      spacing: 8,
                      runSpacing: 8,
                      children: [
                        for (final a in s.actions)
                          OutlinedButton(
                            onPressed: () => _runAction(a),
                            child: Text(a.label),
                          ),
                      ],
                    ),
                  ],
                  SectionHeader(
                    'Resources',
                    trailing: SegmentedButton<bool>(
                      segments: const [
                        ButtonSegment(value: false, label: Text('Memory')),
                        ButtonSegment(value: true, label: Text('CPU')),
                      ],
                      selected: {_cpuChart},
                      onSelectionChanged: (v) =>
                          setState(() => _cpuChart = v.first),
                      style: const ButtonStyle(
                        visualDensity: VisualDensity.compact,
                      ),
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
                ],
              ),
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
    final ports = snap.services
        .where((s) => s.port != null)
        .map((s) => ':${s.port}');
    final parts = [
      snap.running
          ? (snap.uptimeS != null ? 'Up ${_dur(snap.uptimeS!)}' : 'Up')
          : 'Stopped',
      if (snap.pid != null) 'runner · pid ${snap.pid}',
      if (ports.isNotEmpty) ports.join(' '),
    ];
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.only(top: Space.sm, bottom: Space.sm),
      child: Row(
        children: [
          ProjectAvatar(name: snap.name, size: 44),
          const SizedBox(width: Space.md),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    StatusDot(
                      color: snap.running ? c.success : c.faint,
                      pulse: snap.running,
                      size: 7,
                    ),
                    Flexible(
                      child: Text(
                        parts.join(' · '),
                        style: TextStyle(fontSize: 12, color: c.muted),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ],
                ),
                Text(
                  snap.root.replaceFirst(RegExp(r'^/Users/[^/]+'), '~'),
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 11,
                    color: c.faint,
                    fontFamily: monoFamily,
                    fontFamilyFallback: monoFallback,
                  ),
                ),
              ],
            ),
          ),
          if (!snap.running)
            FilledButton.icon(
              onPressed: onUp,
              icon: const Icon(Icons.play_arrow_rounded),
              label: const Text('Start'),
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
        Space.sm,
        Space.xs,
        Space.md,
      ),
      accent: failed ? c.error : null,
      onTap: onLogs,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              StatusDot(
                color: c.serviceState(svc.state),
                pulse: svc.state == 'starting',
                size: 7,
              ),
              const SizedBox(width: 2),
              Text(
                svc.name,
                style: const TextStyle(
                  fontWeight: FontWeight.w600,
                  fontSize: 15,
                ),
              ),
              const SizedBox(width: Space.sm),
              Flexible(
                child: Text(
                  svc.note ?? svc.state,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 12,
                    color: failed ? c.error : c.faint,
                  ),
                ),
              ),
              const Spacer(),
              if (svc.port != null)
                Pill(
                  ':${svc.port}',
                  color: c.accent,
                  mono: true,
                  onTap: () {
                    Clipboard.setData(ClipboardData(text: '${svc.port}'));
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(content: Text('Port ${svc.port} copied')),
                    );
                  },
                ),
              SizedBox(
                width: 40,
                height: 36,
                child: busy
                    ? const Center(
                        child: SizedBox.square(
                          dimension: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        ),
                      )
                    : PopupMenuButton<String>(
                        padding: EdgeInsets.zero,
                        icon: Icon(Icons.more_horiz_rounded, color: c.muted),
                        onSelected: (v) => v == 'logs' ? onLogs() : onOp(v),
                        itemBuilder: (_) => [
                          if (svc.up)
                            const PopupMenuItem(
                              value: 'restart',
                              child: Text('Restart'),
                            ),
                          if (svc.up)
                            const PopupMenuItem(
                              value: 'stop',
                              child: Text('Stop'),
                            ),
                          if (!svc.up)
                            const PopupMenuItem(
                              value: 'start',
                              child: Text('Start'),
                            ),
                          const PopupMenuItem(
                            value: 'logs',
                            child: Text('Logs'),
                          ),
                        ],
                      ),
              ),
            ],
          ),
          if (svc.up) ...[
            const SizedBox(height: Space.xs),
            Padding(
              padding: const EdgeInsets.only(right: Space.sm),
              child: Row(
                children: [
                  Expanded(
                    child: SizedBox(
                      height: 22,
                      child: Sparkline(values: mem, color: color),
                    ),
                  ),
                  const SizedBox(width: Space.sm),
                  Text(
                    '${svc.memMb.round()} MB · ${svc.cpu.round()}%',
                    style: TextStyle(
                      fontSize: 11,
                      color: c.faint,
                      fontFeatures: const [FontFeature.tabularFigures()],
                    ),
                  ),
                ],
              ),
            ),
          ],
          if (svc.lastLine.isNotEmpty) ...[
            const SizedBox(height: Space.xs),
            Padding(
              padding: const EdgeInsets.only(right: Space.sm),
              child: Text(
                svc.lastLine,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontFamily: monoFamily,
                  fontFamilyFallback: monoFallback,
                  fontSize: 11,
                  color: failed ? c.error : c.muted,
                ),
              ),
            ),
          ],
          if (svc.dependsOn.isNotEmpty) ...[
            const SizedBox(height: Space.xs),
            Text(
              'needs ${svc.dependsOn.join(', ')}',
              style: TextStyle(fontSize: 11, color: c.faint),
            ),
          ],
        ],
      ),
    );
  }
}

/// Full-screen logs with search; one service or all merged.
class _LogsPage extends StatefulWidget {
  const _LogsPage({required this.mux, required this.projectId, this.service});
  final Multiplexer? mux;
  final String projectId;
  final String? service;

  @override
  State<_LogsPage> createState() => _LogsPageState();
}

class _LogsPageState extends State<_LogsPage> {
  final _search = TextEditingController();
  String _text = '';
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    final mux = widget.mux;
    if (mux == null) return;
    setState(() => _loading = true);
    final q = _search.text.trim();
    final resp = await mux.call('project_logs', {
      'id': widget.projectId,
      'service': ?widget.service,
      'lines': 300,
      if (q.isNotEmpty) 'query': q,
    });
    if (!mounted) return;
    setState(() {
      _loading = false;
      _text = resp['ok'] == true
          ? (resp['data']?['text'] as String? ?? '')
          : (resp['error'] as String? ?? 'error');
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(
          widget.service == null ? 'Logs' : 'Logs · ${widget.service}',
        ),
        actions: [
          IconButton(onPressed: _load, icon: const Icon(Icons.refresh)),
        ],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(8),
            child: TextField(
              controller: _search,
              decoration: const InputDecoration(
                hintText: 'Search (text or /regex/)',
                prefixIcon: Icon(Icons.search),
                isDense: true,
                border: OutlineInputBorder(),
              ),
              onSubmitted: (_) => _load(),
            ),
          ),
          Expanded(
            child: _loading
                ? const Center(child: CircularProgressIndicator())
                : SingleChildScrollView(
                    reverse: true,
                    padding: const EdgeInsets.all(8),
                    child: SelectableText(
                      _text.isEmpty ? 'No log lines yet.' : _text,
                      style: const TextStyle(
                        fontFamily: monoFamily,
                        fontFamilyFallback: monoFallback,
                        fontSize: 11,
                      ),
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

class _AgentRow extends StatelessWidget {
  const _AgentRow({required this.status, required this.onTap});
  final AgentStatus status;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    final line = status.state == 'working'
        ? (status.prompt ?? '')
        : (status.lastMessage ?? status.prompt ?? '');
    return AtlasCard(
      onTap: onTap,
      accent: status.state == 'blocked' || status.state == 'waiting'
          ? AtlasColors.of(context).warning
          : null,
      child: Row(
        children: [
          StatusDot(
            color: AtlasColors.of(context).agentState(status.state),
            pulse: status.state == 'working' || status.state == 'blocked',
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '${status.agent} · ${agentStateLabel(status.state)}',
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                if (line.isNotEmpty)
                  Text(
                    line.replaceAll('\n', ' '),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(fontSize: 12, color: muted),
                  ),
              ],
            ),
          ),
          Icon(Icons.chevron_right, color: muted),
        ],
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
