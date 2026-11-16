import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/git/branch_bar.dart';
import 'package:atlas_mobile/features/git/diff_panel.dart';
import 'package:atlas_mobile/features/git/git_models.dart';
import 'package:atlas_mobile/features/git/sheets/commit_sheet.dart';
import 'package:atlas_mobile/features/git/sheets/pr_sheet.dart';

/// Review and ship a project's changes from the phone: files with their status, the diff of each,
/// stage toggles, commit (with the AI message the desktop has), push and a pull request. Every
/// action is one fixed hostd command (`git_*`); the phone never sends a git command line.
class ChangesScreen extends ConsumerStatefulWidget {
  const ChangesScreen({super.key, required this.device, required this.project});

  final MacDevice device;
  final ProjectInfo project;

  @override
  ConsumerState<ChangesScreen> createState() => _ChangesScreenState();
}

class _ChangesScreenState extends ConsumerState<ChangesScreen> {
  GitStatus? _status;
  String? _error;
  final Set<String> _busy = {};

  /// Diffs by `staged:path`, loaded when a row is opened.
  final Map<String, FileDiff?> _diffs = {};
  final Set<String> _open = {};
  String? _prUrl;

  String get _root => widget.project.root;

  Multiplexer? get _mux => liveMux(ref, widget.device);

  @override
  void initState() {
    super.initState();
    // The connection may still be opening: wait for it, then load.
    ref.listenManual(connectionNotifierProvider(widget.device), (_, next) {
      if (next.value != null && _status == null && _error == null) _refresh();
    }, fireImmediately: true);
  }

  Future<Map<String, dynamic>?> _call(
    String cmd, [
    Map<String, dynamic> args = const {},
  ]) async {
    final Multiplexer mux;
    try {
      mux = await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable catch (e) {
      // Every caller is something the user pressed: say why nothing happened.
      _toast(e.message);
      return null;
    }
    final r = await mux.call(cmd, {'root': _root, ...args});
    if (!mounted) return null;
    if (r['ok'] != true) {
      _toast(r['error'] as String? ?? 'Failed');
      return null;
    }
    return (r['data'] as Map?)?.cast<String, dynamic>() ?? const {};
  }

  void _toast(String text) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
  }

  Future<void> _refresh() async {
    final mux = _mux;
    if (mux == null) return;
    final r = await mux.call('git_status', {'root': _root});
    if (!mounted) return;
    setState(() {
      if (r['ok'] == true) {
        _status = GitStatus.fromJson((r['data'] as Map).cast());
        _error = null;
        _diffs.clear();
      } else {
        _error = r['error'] as String? ?? 'error';
      }
    });
    // Keep open rows open with fresh diffs.
    for (final key in _open.toList()) {
      final (staged, path) = _split(key);
      _loadDiff(path, staged);
    }
  }

  static (bool, String) _split(String key) =>
      (key.startsWith('s:'), key.substring(2));
  static String _key(bool staged, String path) => '${staged ? 's' : 'u'}:$path';

  Future<void> _loadDiff(String path, bool staged) async {
    final data = await _call('git_diff', {'path': path, 'staged': staged});
    if (data == null || !mounted) return;
    final files = GitDiff.fromJson(data).files;
    setState(
      () => _diffs[_key(staged, path)] = files
          .where((f) => f.path == path)
          .firstOrNull,
    );
  }

  Future<void> _toggleOpen(String path, bool staged) async {
    final k = _key(staged, path);
    if (_open.remove(k)) {
      setState(() {});
      return;
    }
    setState(() => _open.add(k));
    await _loadDiff(path, staged);
  }

  Future<void> _stage(String path, {required bool staged}) async {
    setState(() => _busy.add(path));
    await _call(staged ? 'git_unstage' : 'git_stage', {
      'paths': [path],
    });
    if (mounted) setState(() => _busy.remove(path));
    await _refresh();
  }

  Future<void> _stageAll(List<GitEntry> files) async {
    setState(() => _busy.add('*'));
    await _call('git_stage', {
      'paths': [for (final e in files) e.path],
    });
    if (mounted) setState(() => _busy.remove('*'));
    await _refresh();
  }

  Future<void> _commit() async {
    final msg = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => CommitSheet(
        staged: _status?.staged.length ?? 0,
        suggest: () async {
          final d = await _call('git_commit_message');
          return d?['message'] as String?;
        },
      ),
    );
    if (msg == null || !mounted) return;
    setState(() => _busy.add('commit'));
    final d = await _call('git_commit', {'message': msg});
    if (!mounted) return;
    setState(() => _busy.remove('commit'));
    if (d != null) _toast('Committed ${d['sha'] ?? ''}'.trim());
    await _refresh();
  }

  Future<void> _push() async {
    setState(() => _busy.add('push'));
    final d = await _call('git_push');
    if (!mounted) return;
    setState(() => _busy.remove('push'));
    if (d != null) _toast('Pushed ${d['branch'] ?? ''}'.trim());
    await _refresh();
  }

  Future<void> _openPr() async {
    final input = await showModalBottomSheet<(String, String)>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => PrSheet(branch: _status?.branch ?? ''),
    );
    if (input == null || !mounted) return;
    setState(() => _busy.add('pr'));
    final d = await _call('git_pr_create', {
      'title': input.$1,
      'body': input.$2,
    });
    if (!mounted) return;
    setState(() {
      _busy.remove('pr');
      if (d != null) _prUrl = d['url'] as String?;
    });
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final s = _status;
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.project.name),
        actions: [
          IconButton(
            tooltip: 'Refresh',
            onPressed: _refresh,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: s == null
          ? (_error != null
                ? EmptyState(
                    title: 'Could not read the changes',
                    message: _error!,
                    action: OutlinedButton(
                      onPressed: _refresh,
                      child: const Text('Try again'),
                    ),
                  )
                : const Center(child: CircularProgressIndicator()))
          : RefreshIndicator(
              color: c.accent,
              onRefresh: _refresh,
              child: ListView(
                padding: const EdgeInsets.fromLTRB(
                  Space.lg,
                  Space.sm,
                  Space.lg,
                  Space.xxl,
                ),
                children: [
                  BranchBar(status: s),
                  if (s.staged.isNotEmpty) ...[
                    SectionHeader('Staged', count: s.staged.length),
                    for (final e in s.staged) _row(e, staged: true),
                  ],
                  if (s.unstaged.isNotEmpty) ...[
                    SectionHeader(
                      'Changes',
                      count: s.unstaged.length,
                      trailing: TextButton(
                        key: const ValueKey('stage-all'),
                        onPressed: _busy.contains('*')
                            ? null
                            : () => _stageAll(s.unstaged),
                        child: const Text('Stage all'),
                      ),
                    ),
                    for (final e in s.unstaged) _row(e, staged: false),
                  ],
                  if (s.entries.isEmpty)
                    const Padding(
                      padding: EdgeInsets.only(top: Space.xxl),
                      child: EmptyState(
                        title: 'Nothing to commit',
                        message: 'The working tree is clean.',
                      ),
                    ),
                  const SizedBox(height: Space.lg),
                  _actions(s),
                ],
              ),
            ),
    );
  }

  Widget _row(GitEntry e, {required bool staged}) {
    final c = AtlasColors.of(context);
    final k = _key(staged, e.path);
    final open = _open.contains(k);
    final badge = e.badge(staged: staged);
    return Column(
      key: ValueKey('row-$k'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        AtlasCard(
          onTap: () => _toggleOpen(e.path, staged),
          padding: const EdgeInsets.fromLTRB(
            Space.md,
            Space.xs,
            Space.xs,
            Space.xs,
          ),
          child: Row(
            children: [
              SizedBox(
                width: 22,
                child: Text(
                  badge,
                  style: TextStyle(
                    color: switch (badge) {
                      'D' => c.error,
                      'M' => c.warning,
                      _ => c.success,
                    },
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ),
              Expanded(
                child: Text(
                  e.path,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontWeight: FontWeight.w500),
                ),
              ),
              IconButton(
                key: ValueKey('${staged ? 'unstage' : 'stage'}-${e.path}'),
                tooltip: staged ? 'Unstage' : 'Stage',
                onPressed: _busy.contains(e.path)
                    ? null
                    : () => _stage(e.path, staged: staged),
                icon: Icon(
                  staged
                      ? Icons.check_box_rounded
                      : Icons.check_box_outline_blank_rounded,
                  color: staged ? c.accent : null,
                ),
              ),
            ],
          ),
        ),
        if (open) DiffPanel(diff: _diffs[k], loaded: _diffs.containsKey(k)),
      ],
    );
  }

  Widget _actions(GitStatus s) {
    final committing = _busy.contains('commit');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        FilledButton.icon(
          key: const ValueKey('commit-button'),
          onPressed: s.staged.isEmpty || committing ? null : _commit,
          icon: const Icon(Icons.check),
          label: Text(
            s.staged.isEmpty
                ? 'Stage files to commit'
                : 'Commit ${s.staged.length} file${s.staged.length == 1 ? '' : 's'}',
          ),
        ),
        if (s.needsPush && s.entries.every((e) => !e.hasStaged)) ...[
          const SizedBox(height: Space.sm),
          OutlinedButton.icon(
            key: const ValueKey('push-button'),
            onPressed: _busy.contains('push') ? null : _push,
            icon: const Icon(Icons.north_east),
            label: Text(
              s.upstream == null
                  ? 'Push ${s.branch}'
                  : 'Push ${s.ahead} commit${s.ahead == 1 ? '' : 's'}',
            ),
          ),
        ],
        if (s.upstream != null && s.ahead == 0) ...[
          const SizedBox(height: Space.sm),
          if (_prUrl != null)
            SelectableText(_prUrl!, textAlign: TextAlign.center)
          else
            OutlinedButton.icon(
              key: const ValueKey('pr-button'),
              onPressed: _busy.contains('pr') ? null : _openPr,
              icon: const Icon(Icons.call_merge),
              label: const Text('Open a pull request'),
            ),
        ],
      ],
    );
  }
}
