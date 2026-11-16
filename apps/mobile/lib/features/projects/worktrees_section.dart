import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/projects/worktree.dart';

/// The project's worktrees: each with its branch and whether it has changes, a button to make a new one and
/// one to remove a linked one. Removal never forces: a worktree with unsaved work stays and says why.
class WorktreesSection extends ConsumerStatefulWidget {
  const WorktreesSection({
    super.key,
    required this.device,
    required this.projectId,
  });

  final MacDevice device;
  final String projectId;

  @override
  ConsumerState<WorktreesSection> createState() => _WorktreesSectionState();
}

class _WorktreesSectionState extends ConsumerState<WorktreesSection> {
  bool _busy = false;
  String? _notice;

  ({MacDevice device, String projectId}) get _key =>
      (device: widget.device, projectId: widget.projectId);

  Future<Map<String, dynamic>?> _call(
    String cmd,
    Map<String, Object?> args,
  ) async {
    final Multiplexer mux;
    try {
      mux = await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable catch (e) {
      if (mounted) setState(() => _notice = e.message);
      return null;
    }
    setState(() {
      _busy = true;
      _notice = null;
    });
    final resp = await mux.call(cmd, args);
    if (!mounted) return null;
    setState(() => _busy = false);
    if (resp['ok'] != true) {
      setState(() => _notice = resp['error'] as String? ?? 'Failed');
      return null;
    }
    return (resp['data'] as Map).cast<String, dynamic>();
  }

  Future<void> _create() async {
    final name = await showDialog<String>(
      context: context,
      builder: (_) => const _NameDialog(),
    );
    if (name == null) return;
    HapticFeedback.lightImpact();
    final data = await _call('worktree_create', {
      'project': widget.projectId,
      if (name.isNotEmpty) 'name': name,
    });
    if (data == null) return;
    final made = CreatedWorktree.fromJson(data);
    setState(
      () => _notice = made.setup != null && !made.setup!.ok
          ? '${made.worktree.name} created, but its setup command failed:\n${made.setup!.output.trim()}'
          : null,
    );
    ref.invalidate(worktreesProvider(_key));
  }

  Future<void> _remove(Worktree w) async {
    final go = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Remove ${w.name}?'),
        content: const Text(
          'Its folder is deleted. The branch stays, so you can bring it back. '
          'A worktree with unsaved work is kept.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
    if (go != true) return;
    final data = await _call('worktree_remove', {'id': w.id});
    if (data == null) return;
    final r = WorktreeRemoval.fromJson(data);
    setState(
      () => _notice = r.removed
          ? null
          : '${w.name} was kept. ${keptReason(r.kept)}',
    );
    ref.invalidate(worktreesProvider(_key));
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final list = ref.watch(worktreesProvider(_key));
    final rows = list.value?.worktrees ?? const <Worktree>[];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SectionHeader(
          'Worktrees',
          count: rows.isEmpty ? null : rows.length,
          trailing: TextButton.icon(
            onPressed: _busy ? null : _create,
            icon: const Icon(Icons.add_rounded, size: 18),
            label: const Text('New worktree'),
          ),
        ),
        if (list.hasError && rows.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: Space.md),
            child: Text('${list.error}', style: TextStyle(color: c.error)),
          )
        else if (rows.isEmpty)
          const Padding(
            padding: EdgeInsets.symmetric(vertical: Space.md),
            child: Center(child: CircularProgressIndicator(strokeWidth: 2)),
          )
        else
          AtlasCard(
            padding: EdgeInsets.zero,
            child: Column(
              children: [
                for (final w in rows)
                  _WorktreeRow(
                    worktree: w,
                    onRemove: _busy || w.main ? null : () => _remove(w),
                  ),
              ],
            ),
          ),
        if (_notice != null)
          Padding(
            padding: const EdgeInsets.only(top: Space.sm),
            child: Text(
              _notice!,
              style: Theme.of(context).textTheme.bodyMedium!
                  .copyWith(color: c.warning),
            ),
          ),
      ],
    );
  }
}

class _WorktreeRow extends StatelessWidget {
  const _WorktreeRow({required this.worktree, required this.onRemove});
  final Worktree worktree;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final w = worktree;
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Space.md,
        vertical: Space.sm,
      ),
      child: Row(
        children: [
          Icon(
            w.main ? Icons.home_outlined : Icons.fork_right_rounded,
            size: 18,
            color: c.muted,
          ),
          const SizedBox(width: Space.sm + 2),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  w.main ? 'main' : w.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                Text(
                  [
                    if (w.branch != null) w.branch!,
                    if (w.main) 'main checkout',
                    if (!w.exists) 'folder missing',
                  ].join(' · '),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.bodySmall!
                      .copyWith(color: c.muted),
                ),
              ],
            ),
          ),
          if (w.dirty)
            Padding(
              padding: const EdgeInsets.only(right: Space.sm),
              child: Text(
                'changes',
                style: Theme.of(context).textTheme.bodySmall!
                    .copyWith(color: c.warning),
              ),
            ),
          if (!w.main)
            IconButton(
              tooltip: 'Remove ${w.name}',
              onPressed: onRemove,
              icon: const Icon(Icons.delete_outline_rounded, size: 20),
            ),
        ],
      ),
    );
  }
}

class _NameDialog extends StatefulWidget {
  const _NameDialog();

  @override
  State<_NameDialog> createState() => _NameDialogState();
}

class _NameDialogState extends State<_NameDialog> {
  final _name = TextEditingController();

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('New worktree'),
    content: TextField(
      controller: _name,
      autofocus: true,
      textCapitalization: TextCapitalization.none,
      decoration: const InputDecoration(
        hintText: 'Name (optional)',
        helperText: 'Empty picks a name for you.',
      ),
      onSubmitted: (v) => Navigator.pop(context, v.trim()),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () => Navigator.pop(context, _name.text.trim()),
        child: const Text('Create'),
      ),
    ],
  );
}
