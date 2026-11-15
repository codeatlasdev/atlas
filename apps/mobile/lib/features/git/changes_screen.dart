import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/markdown/code_block.dart';
import 'package:atlas_mobile/features/git/git_models.dart';

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

  Multiplexer? get _mux =>
      ref.read(connectionNotifierProvider(widget.device)).value;

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
    final mux = _mux;
    if (mux == null) return null;
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
      builder: (_) => _CommitSheet(
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
      builder: (_) => _PrSheet(branch: _status?.branch ?? ''),
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
                  _BranchBar(status: s),
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
        if (open) _DiffPanel(diff: _diffs[k], loaded: _diffs.containsKey(k)),
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

class _BranchBar extends StatelessWidget {
  const _BranchBar({required this.status});
  final GitStatus status;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Row(
      children: [
        Icon(Icons.call_split, size: 18, color: c.accent),
        const SizedBox(width: Space.sm),
        Flexible(
          child: Text(
            status.branch,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontWeight: FontWeight.w700),
          ),
        ),
        if (status.upstream != null) ...[
          const SizedBox(width: Space.sm),
          Text(
            '${status.ahead > 0 ? '↑${status.ahead} ' : ''}${status.upstream}',
            style: TextStyle(color: c.muted, fontSize: 12),
          ),
        ] else ...[
          const SizedBox(width: Space.sm),
          Text('not pushed', style: TextStyle(color: c.muted, fontSize: 12)),
        ],
      ],
    );
  }
}

class _DiffPanel extends StatelessWidget {
  const _DiffPanel({required this.diff, required this.loaded});
  final FileDiff? diff;
  final bool loaded;

  @override
  Widget build(BuildContext context) {
    final d = diff;
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.sm),
      child: !loaded
          ? const Padding(
              padding: EdgeInsets.all(Space.md),
              child: Center(child: CircularProgressIndicator()),
            )
          : d == null || d.binary || d.hunks.isEmpty
          ? const Padding(
              padding: EdgeInsets.all(Space.md),
              child: Text('No text changes to show (binary or empty).'),
            )
          : AtlasCodeBlock(code: d.asPatch(), language: 'diff', fontSize: 12),
    );
  }
}

class _CommitSheet extends StatefulWidget {
  const _CommitSheet({required this.staged, required this.suggest});
  final int staged;
  final Future<String?> Function() suggest;

  @override
  State<_CommitSheet> createState() => _CommitSheetState();
}

class _CommitSheetState extends State<_CommitSheet> {
  final _text = TextEditingController();
  bool _writing = false;

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  Future<void> _ai() async {
    setState(() => _writing = true);
    final m = await widget.suggest();
    if (!mounted) return;
    setState(() {
      _writing = false;
      if (m != null) _text.text = m;
    });
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: EdgeInsets.fromLTRB(
      Space.lg,
      0,
      Space.lg,
      MediaQuery.viewInsetsOf(context).bottom + Space.lg,
    ),
    child: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'Commit ${widget.staged} staged file${widget.staged == 1 ? '' : 's'}',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        const SizedBox(height: Space.md),
        TextField(
          key: const ValueKey('commit-message'),
          controller: _text,
          minLines: 3,
          maxLines: 8,
          onChanged: (_) => setState(() {}),
          decoration: const InputDecoration(
            hintText: 'Subject, blank line, then why',
            border: OutlineInputBorder(),
          ),
        ),
        const SizedBox(height: Space.sm),
        Wrap(
          alignment: WrapAlignment.spaceBetween,
          crossAxisAlignment: WrapCrossAlignment.center,
          spacing: Space.sm,
          runSpacing: Space.sm,
          children: [
            OutlinedButton.icon(
              key: const ValueKey('ai-message'),
              onPressed: _writing ? null : _ai,
              icon: _writing
                  ? const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.auto_awesome),
              label: Text(_writing ? 'Writing…' : 'Write with AI'),
            ),
            FilledButton(
              key: const ValueKey('commit-confirm'),
              onPressed: _text.text.trim().isEmpty
                  ? null
                  : () => Navigator.pop(context, _text.text.trim()),
              child: const Text('Commit'),
            ),
          ],
        ),
      ],
    ),
  );
}

class _PrSheet extends StatefulWidget {
  const _PrSheet({required this.branch});
  final String branch;

  @override
  State<_PrSheet> createState() => _PrSheetState();
}

class _PrSheetState extends State<_PrSheet> {
  final _title = TextEditingController();
  final _body = TextEditingController();

  @override
  void dispose() {
    _title.dispose();
    _body.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: EdgeInsets.fromLTRB(
      Space.lg,
      0,
      Space.lg,
      MediaQuery.viewInsetsOf(context).bottom + Space.lg,
    ),
    child: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'Pull request from ${widget.branch}',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        const SizedBox(height: Space.md),
        TextField(
          key: const ValueKey('pr-title'),
          controller: _title,
          onChanged: (_) => setState(() {}),
          decoration: const InputDecoration(
            labelText: 'Title',
            border: OutlineInputBorder(),
          ),
        ),
        const SizedBox(height: Space.sm),
        TextField(
          key: const ValueKey('pr-body'),
          controller: _body,
          minLines: 3,
          maxLines: 8,
          decoration: const InputDecoration(
            labelText: 'Description',
            border: OutlineInputBorder(),
          ),
        ),
        const SizedBox(height: Space.md),
        FilledButton(
          key: const ValueKey('pr-confirm'),
          onPressed: _title.text.trim().isEmpty
              ? null
              : () => Navigator.pop(context, (
                  _title.text.trim(),
                  _body.text.trim(),
                )),
          child: const Text('Open pull request'),
        ),
      ],
    ),
  );
}
