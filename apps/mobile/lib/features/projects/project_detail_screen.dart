import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/projects/charts.dart';
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
          ? Center(
              child: _error != null
                  ? SelectableText(_error!)
                  : const CircularProgressIndicator(),
            )
          : RefreshIndicator(
              onRefresh: _refresh,
              child: ListView(
                padding: const EdgeInsets.fromLTRB(12, 4, 12, 24),
                children: [
                  _Header(
                    snap: s,
                    onUp: _busy.contains('up') ? null : () => _serviceOp('up'),
                  ),
                  if (s.setupState != null) _SetupRow(snap: s),
                  const SizedBox(height: 8),
                  _Kpis(snap: s),
                  const SizedBox(height: 16),
                  _section(context, 'Services ${s.services.length}'),
                  for (var i = 0; i < s.services.length; i++)
                    _ServiceCard(
                      svc: s.services[i],
                      mem: _history.mem[s.services[i].name] ?? const [],
                      color: seriesColors[i % seriesColors.length],
                      busy: _busy.contains(s.services[i].name),
                      onOp: (op) => _serviceOp(op, service: s.services[i].name),
                      onLogs: () => _openLogs(service: s.services[i].name),
                    ),
                  if (s.agents.isNotEmpty) ...[
                    const SizedBox(height: 8),
                    _section(context, 'Agents ${s.agents.length}'),
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
                    _section(context, 'Actions'),
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
                  const SizedBox(height: 16),
                  Row(
                    children: [
                      _section(context, 'Resources'),
                      const Spacer(),
                      SegmentedButton<bool>(
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
                    ],
                  ),
                  _Card(
                    child: LinesChart(
                      series: _cpuChart ? _history.cpu : _history.mem,
                      unit: _cpuChart ? '%' : 'MB',
                    ),
                  ),
                  const SizedBox(height: 16),
                  _section(context, 'Health over time'),
                  _Card(child: HealthTimeline(states: _history.states)),
                ],
              ),
            ),
    );
  }
}

Widget _section(BuildContext context, String title) => Padding(
  padding: const EdgeInsets.only(bottom: 8, top: 4),
  child: Text(title, style: Theme.of(context).textTheme.titleSmall),
);

class _Card extends StatelessWidget {
  const _Card({required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context) => Container(
    width: double.infinity,
    margin: const EdgeInsets.only(bottom: 8),
    padding: const EdgeInsets.all(12),
    decoration: BoxDecoration(
      color: Theme.of(context).colorScheme.surfaceContainerHighest
          .withValues(alpha: 0.4),
      borderRadius: BorderRadius.circular(10),
      border: Border.all(
        color: Theme.of(context).colorScheme.outlineVariant
            .withValues(alpha: 0.4),
      ),
    ),
    child: child,
  );
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
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
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
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  parts.join(' · '),
                  style: TextStyle(fontSize: 12, color: muted),
                ),
              ),
              if (!snap.running)
                FilledButton.icon(
                  onPressed: onUp,
                  icon: const Icon(Icons.play_arrow),
                  label: const Text('Start stack'),
                ),
            ],
          ),
          Text(
            snap.root.replaceFirst(RegExp(r'^/Users/[^/]+'), '~'),
            style: TextStyle(
              fontSize: 11,
              color: muted,
              fontFamily: 'monospace',
            ),
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
    return _Card(
      child: Row(
        children: [
          Icon(
            ok
                ? Icons.check
                : snap.setupState == 'running'
                ? Icons.hourglass_top
                : Icons.error_outline,
            size: 16,
            color: ok ? stateColor('running') : stateColor('failed'),
          ),
          const SizedBox(width: 8),
          const Text('Setup', style: TextStyle(fontWeight: FontWeight.w600)),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              snap.setupCommand ?? '',
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
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
    final mem = snap.memMb >= 1024
        ? '${(snap.memMb / 1024).toStringAsFixed(1)} GB'
        : '${snap.memMb.round()} MB';
    final items = [
      (
        Icons.dns_outlined,
        'Services',
        '${snap.upCount}/${snap.services.length}',
        'running',
      ),
      (Icons.refresh, 'Restarts', '${snap.restarts}', 'this run'),
      (Icons.warning_amber, 'Errors', '${snap.errors5m}', 'last 5 min'),
      (Icons.memory, 'Memory', mem, 'all services'),
      (Icons.speed, 'CPU', '${snap.cpu.round()}%', 'of one core'),
    ];
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    // Two per row, height from content: a fixed aspect ratio overflowed with larger text.
    return LayoutBuilder(
      builder: (context, box) => Wrap(
        spacing: 8,
        children: [
          for (final (icon, title, value, sub) in items)
            SizedBox(
              width: (box.maxWidth - 8) / 2,
              child: _Card(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Icon(icon, size: 13, color: muted),
                        const SizedBox(width: 4),
                        Text(
                          title,
                          style: TextStyle(fontSize: 11, color: muted),
                        ),
                      ],
                    ),
                    const SizedBox(height: 6),
                    Text(
                      value,
                      style: const TextStyle(
                        fontSize: 20,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    Text(sub, style: TextStyle(fontSize: 10, color: muted)),
                  ],
                ),
              ),
            ),
        ],
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
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    return _Card(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 10,
                height: 10,
                decoration: BoxDecoration(
                  color: stateColor(svc.state),
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 8),
              Text(
                svc.name,
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
              const SizedBox(width: 8),
              Text(
                svc.note ?? svc.state,
                style: TextStyle(fontSize: 12, color: muted),
              ),
              const Spacer(),
              if (svc.port != null)
                InkWell(
                  onTap: () =>
                      Clipboard.setData(ClipboardData(text: '${svc.port}')),
                  child: Text(
                    ':${svc.port}',
                    style: TextStyle(
                      fontSize: 12,
                      color: Theme.of(context).colorScheme.primary,
                    ),
                  ),
                ),
              busy
                  ? const Padding(
                      padding: EdgeInsets.all(12),
                      child: SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      ),
                    )
                  : PopupMenuButton<String>(
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
                        const PopupMenuItem(value: 'logs', child: Text('Logs')),
                      ],
                    ),
            ],
          ),
          if (svc.up) ...[
            Row(
              children: [
                Expanded(
                  child: Sparkline(values: mem, color: color),
                ),
                const SizedBox(width: 8),
                Text(
                  '${svc.memMb.round()} MB · ${svc.cpu.round()}%',
                  style: TextStyle(fontSize: 11, color: muted),
                ),
              ],
            ),
            const SizedBox(height: 6),
          ],
          if (svc.lastLine.isNotEmpty)
            Text(
              svc.lastLine,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontFamily: 'monospace',
                fontSize: 11,
                color: muted,
              ),
            ),
          if (svc.dependsOn.isNotEmpty) ...[
            const SizedBox(height: 6),
            Wrap(
              spacing: 6,
              children: [
                for (final d in svc.dependsOn)
                  Text('↳ $d', style: TextStyle(fontSize: 11, color: muted)),
              ],
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
                        fontFamily: 'monospace',
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
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(10),
      child: _Card(
        child: Row(
          children: [
            Container(
              width: 10,
              height: 10,
              decoration: BoxDecoration(
                color: agentStateColor(status.state),
                shape: BoxShape.circle,
              ),
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
      ),
    );
  }
}
