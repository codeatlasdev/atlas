import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';

/// One checkout of a project from `worktree_list`: its main checkout or a linked worktree.
class Worktree {
  const Worktree({
    required this.id,
    required this.path,
    required this.name,
    required this.branch,
    required this.main,
    required this.dirty,
    required this.exists,
    required this.portOffset,
  });

  final String id;
  final String path;
  final String name;
  final String? branch;
  final bool main;

  /// Uncommitted or untracked changes (read from git when listed).
  final bool dirty;

  /// The folder is still on disk (a deleted one is only a registry entry).
  final bool exists;
  final int portOffset;

  factory Worktree.fromJson(Map<String, dynamic> j) => Worktree(
    id: j['id'] as String? ?? '',
    path: j['path'] as String? ?? '',
    name: j['name'] as String? ?? '',
    branch: j['branch'] as String?,
    main: j['main'] as bool? ?? false,
    dirty: j['dirty'] as bool? ?? false,
    exists: j['exists'] as bool? ?? true,
    portOffset: (j['portOffset'] as num?)?.toInt() ?? 0,
  );
}

/// `worktree_list`: the repo's own id and its checkouts, main first.
class WorktreeList {
  const WorktreeList({required this.repoId, required this.worktrees});
  final String repoId;
  final List<Worktree> worktrees;

  factory WorktreeList.fromJson(Map<String, dynamic> j) => WorktreeList(
    repoId: j['repoId'] as String? ?? '',
    worktrees: [
      for (final w in (j['worktrees'] as List<dynamic>?) ?? const [])
        Worktree.fromJson((w as Map).cast<String, dynamic>()),
    ],
  );
}

/// What `worktree.setup` of atlas.yaml did, when the project has one.
class SetupResult {
  const SetupResult({required this.ok, required this.output});
  final bool ok;
  final String output;
}

/// `worktree_create`: the new checkout and the outcome of its setup command.
class CreatedWorktree {
  const CreatedWorktree({required this.worktree, this.setup});
  final Worktree worktree;
  final SetupResult? setup;

  factory CreatedWorktree.fromJson(Map<String, dynamic> j) {
    final s = j['setup'];
    return CreatedWorktree(
      worktree: Worktree.fromJson(
        (j['worktree'] as Map).cast<String, dynamic>(),
      ),
      setup: s is Map
          ? SetupResult(
              ok: s['ok'] == true,
              output: s['output'] as String? ?? '',
            )
          : null,
    );
  }
}

/// `worktree_remove`: removed, or kept because removing would lose work.
class WorktreeRemoval {
  const WorktreeRemoval({required this.removed, this.kept});
  final bool removed;

  /// `dirty` | `secrets` | `unpushed`.
  final String? kept;

  factory WorktreeRemoval.fromJson(Map<String, dynamic> j) => WorktreeRemoval(
    removed: j['removed'] == true,
    kept: j['kept'] as String?,
  );
}

/// Why a worktree was kept, in words for the person holding the phone.
String keptReason(String? why) => switch (why) {
  'dirty' => 'It has uncommitted changes. Commit or discard them on your computer first.',
  'secrets' => 'It holds gitignored .env files that would be deleted with it.',
  'unpushed' => 'It has commits that are not pushed to any remote yet.',
  _ => 'It cannot be removed without losing work.',
};

/// The checkouts of project [projectId] on the connected computer.
final worktreesProvider = FutureProvider.autoDispose
    .family<WorktreeList, ({MacDevice device, String projectId})>((
      ref,
      key,
    ) async {
      final mux = await ref.watch(
        connectionNotifierProvider(key.device).future,
      );
      if (mux == null) throw 'Not connected to your computer';
      final resp = await mux.call('worktree_list', {'project': key.projectId});
      if (resp['ok'] != true) {
        throw resp['error'] as String? ?? 'Could not list worktrees';
      }
      return WorktreeList.fromJson(
        (resp['data'] as Map).cast<String, dynamic>(),
      );
    });
