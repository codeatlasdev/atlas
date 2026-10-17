import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';

/// Detail screen for a single project: services, actions, and logs.
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

class _ProjectDetailScreenState extends ConsumerState<ProjectDetailScreen> {
  String _logs = '';
  bool _logsLoading = false;
  final TextEditingController _logSearchCtrl = TextEditingController();

  @override
  void dispose() {
    _logSearchCtrl.dispose();
    super.dispose();
  }

  Multiplexer? get _mux =>
      ref.read(connectionNotifierProvider(widget.device)).value;

  // ── Services ────────────────────────────────────────────────────────────────

  Future<void> _serviceOp(ServiceInfo service, String op) async {
    final mux = _mux;
    if (mux == null) return;
    await mux.call('project_service', {
      'id': widget.project.id,
      'service': service.name,
      'op': op,
    });
    ref.invalidate(projectListProvider(widget.device));
  }

  void _showServiceSheet(BuildContext ctx, ServiceInfo service) {
    showModalBottomSheet<void>(
      context: ctx,
      builder: (_) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.play_arrow),
              title: const Text('Start'),
              onTap: () {
                Navigator.pop(ctx);
                _serviceOp(service, 'start');
              },
            ),
            ListTile(
              leading: const Icon(Icons.stop),
              title: const Text('Stop'),
              onTap: () {
                Navigator.pop(ctx);
                _serviceOp(service, 'stop');
              },
            ),
            ListTile(
              leading: const Icon(Icons.refresh),
              title: const Text('Restart'),
              onTap: () {
                Navigator.pop(ctx);
                _serviceOp(service, 'restart');
              },
            ),
            ListTile(
              leading: const Icon(Icons.arrow_upward),
              title: const Text('Up'),
              onTap: () {
                Navigator.pop(ctx);
                _serviceOp(service, 'up');
              },
            ),
          ],
        ),
      ),
    );
  }

  // ── Actions ─────────────────────────────────────────────────────────────────

  Future<void> _runAction(String key, {bool confirmed = false}) async {
    final mux = _mux;
    if (mux == null) return;
    final resp = await mux.call('project_action', {
      'id': widget.project.id,
      'key': key,
      'confirmed': confirmed,
    });

    final ok = resp['ok'] as bool;
    final text = ok
        ? (resp['data']?['text'] as String? ?? '')
        : (resp['error'] as String? ?? '');

    final needsConfirm = text.toLowerCase().contains('confirm');
    if (!ok && needsConfirm && !confirmed && mounted) {
      final go = await showDialog<bool>(
        context: context,
        builder: (_) => AlertDialog(
          title: const Text('Confirm action'),
          content: Text(text),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Confirm'),
            ),
          ],
        ),
      );
      if (go == true) return _runAction(key, confirmed: true);
      return;
    }

    if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(text.isEmpty ? (ok ? 'Done.' : 'Error.') : text)),
      );
    }
  }

  void _promptRunAction(BuildContext ctx) async {
    final keyCtrl = TextEditingController();
    final key = await showDialog<String>(
      context: ctx,
      builder: (_) => AlertDialog(
        title: const Text('Run action'),
        content: TextField(
          controller: keyCtrl,
          decoration: const InputDecoration(labelText: 'Action key'),
          autofocus: true,
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, keyCtrl.text.trim()),
            child: const Text('Run'),
          ),
        ],
      ),
    );
    keyCtrl.dispose();
    if (key != null && key.isNotEmpty) _runAction(key);
  }

  // ── Logs ────────────────────────────────────────────────────────────────────

  Future<void> _fetchLogs({String? query}) async {
    final mux = _mux;
    if (mux == null) return;
    setState(() => _logsLoading = true);
    final args = <String, dynamic>{'id': widget.project.id, 'lines': 200};
    if (query != null && query.isNotEmpty) args['query'] = query;
    final resp = await mux.call('project_logs', args);
    setState(() {
      _logs = resp['ok'] == true
          ? (resp['data']?['text'] as String? ?? '')
          : (resp['error'] as String? ?? 'Error fetching logs.');
      _logsLoading = false;
    });
  }

  void _showLogsSheet(BuildContext ctx) {
    _fetchLogs();
    showModalBottomSheet<void>(
      context: ctx,
      isScrollControlled: true,
      builder: (_) => StatefulBuilder(
        builder: (ctx2, setSheetState) => DraggableScrollableSheet(
          expand: false,
          initialChildSize: 0.7,
          minChildSize: 0.3,
          maxChildSize: 0.95,
          builder: (_, sc) => Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              children: [
                TextField(
                  controller: _logSearchCtrl,
                  decoration: InputDecoration(
                    labelText: 'Search logs',
                    suffixIcon: IconButton(
                      icon: const Icon(Icons.search),
                      onPressed: () => _fetchLogs(query: _logSearchCtrl.text),
                    ),
                  ),
                  onSubmitted: (q) => _fetchLogs(query: q),
                ),
                const SizedBox(height: 8),
                Expanded(
                  child: _logsLoading
                      ? const Center(child: CircularProgressIndicator())
                      : SingleChildScrollView(
                          controller: sc,
                          child: SelectableText(
                            _logs,
                            style: const TextStyle(
                              fontFamily: 'monospace',
                              fontSize: 12,
                            ),
                          ),
                        ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  // ── Build ────────────────────────────────────────────────────────────────────

  Color _stateColor(String state) => switch (state) {
        'running' => Colors.green,
        'stopped' => Colors.red,
        'starting' => Colors.yellow.shade700,
        'failed' => Colors.red.shade800,
        _ => Colors.grey,
      };

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(widget.project.name)),
      body: ListView(
        padding: const EdgeInsets.symmetric(vertical: 8),
        children: [
          // ── Services ────────────────────────────────────────────────────────
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
            child: Text('Services',
                style: Theme.of(context).textTheme.titleSmall),
          ),
          for (final svc in widget.project.services)
            ListTile(
              leading: CircleAvatar(
                backgroundColor: _stateColor(svc.state),
                radius: 6,
                child: const SizedBox.shrink(),
              ),
              title: Text(svc.name),
              subtitle: Text(
                [
                  svc.state,
                  if (svc.port != null) ':${svc.port}',
                  if (svc.health != null) ' (${svc.health})',
                ].join(),
              ),
              trailing: const Icon(Icons.more_vert),
              onLongPress: () => _showServiceSheet(context, svc),
              onTap: () => _showServiceSheet(context, svc),
            ),

          // ── Actions ──────────────────────────────────────────────────────────
          const Divider(),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
            child: Text('Actions',
                style: Theme.of(context).textTheme.titleSmall),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
            child: OutlinedButton.icon(
              icon: const Icon(Icons.play_circle_outline),
              label: const Text('Run Action\u2026'),
              onPressed: () => _promptRunAction(context),
            ),
          ),

          // ── Logs ─────────────────────────────────────────────────────────────
          const Divider(),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
            child: Text('Logs', style: Theme.of(context).textTheme.titleSmall),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
            child: OutlinedButton.icon(
              icon: const Icon(Icons.terminal),
              label: const Text('View Logs'),
              onPressed: () => _showLogsSheet(context),
            ),
          ),
          const SizedBox(height: 16),
        ],
      ),
    );
  }
}
