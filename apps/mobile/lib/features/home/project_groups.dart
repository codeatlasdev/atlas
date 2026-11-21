/// How the home shows a computer's project registry. The desktop registers every folder it opened, every worktree
/// as its own entry and plain folders opened once to edit a file (`~/Downloads`), so a flat list buried the real
/// projects (2026-11-20: 220 worktree entries of one repository). Here:
///
/// - one row per repository: the main checkout heads it, its worktrees hang under it;
/// - a worktree that is doing something (needs you, runs, has an agent) is always shown under its project, the idle
///   ones fold behind "N worktrees";
/// - folders that are not projects go to a collapsed "Other folders" section at the end;
/// - a search looks through everything, flat.
///
/// An older hostd sends no `repo`/`kind`: every entry is its own project, as before.
library;

import 'package:atlas_mobile/core/protocol/models.dart';

/// A repository on the home: its main checkout (or, when that is not registered, its most active worktree) and the
/// other checkouts of it.
class ProjectGroup {
  ProjectGroup(this.head, this.worktrees);
  final ProjectInfo head;

  /// The repository's other checkouts, most active first.
  final List<ProjectInfo> worktrees;

  /// A key that stays the same across polls (expanded state survives a refresh).
  String get key => head.repo ?? head.root;
}

class HomeProjects {
  const HomeProjects(this.groups, this.folders);
  final List<ProjectGroup> groups;

  /// Folders that are not projects, most active first.
  final List<ProjectInfo> folders;
}

/// 0 needs you, 1 runs, 2 has an agent, 3 idle.
int activity(ProjectInfo p, Set<String> flagged) => flagged.contains(p.id)
    ? 0
    : p.running
    ? 1
    : p.agents.isNotEmpty
    ? 2
    : 3;

/// Whether a worktree shows under its project without being unfolded.
bool isActive(ProjectInfo p, Set<String> flagged) => activity(p, flagged) < 3;

int _byActivityThenName(ProjectInfo a, ProjectInfo b, Set<String> flagged) {
  final r = activity(a, flagged).compareTo(activity(b, flagged));
  if (r != 0) return r;
  return _label(a).compareTo(_label(b));
}

String _label(ProjectInfo p) =>
    (p.worktree ? p.branch ?? p.name : p.name).toLowerCase();

/// Groups [list] for the home. [flagged]: ids of projects that need the user (agent prompts, failed services).
HomeProjects groupProjects(
  List<ProjectInfo> list, {
  Set<String> flagged = const {},
}) {
  final byRepo = <String, List<ProjectInfo>>{};
  final folders = <ProjectInfo>[];
  for (final p in list) {
    if (!p.isProject) {
      folders.add(p);
    } else {
      (byRepo[p.repo ?? p.root] ??= []).add(p);
    }
  }
  final groups = [
    for (final members in byRepo.values) _group(members, flagged),
  ];
  int rank(ProjectGroup g) => [
    g.head,
    ...g.worktrees,
  ].map((p) => activity(p, flagged)).reduce((a, b) => a < b ? a : b);
  groups.sort((a, b) {
    final r = rank(a).compareTo(rank(b));
    if (r != 0) return r;
    // Pinned in the desktop: first among equals.
    if (a.head.pinned != b.head.pinned) return a.head.pinned ? -1 : 1;
    return a.head.name.toLowerCase().compareTo(b.head.name.toLowerCase());
  });
  folders.sort((a, b) => _byActivityThenName(a, b, flagged));
  return HomeProjects(groups, folders);
}

ProjectGroup _group(List<ProjectInfo> members, Set<String> flagged) {
  final sorted = [...members]
    ..sort((a, b) => _byActivityThenName(a, b, flagged));
  // The main checkout heads its repository; without one registered, the most active checkout does.
  final head = sorted.firstWhere(
    (p) => !p.worktree && (p.repo == null || p.repo == p.root),
    orElse: () =>
        sorted.firstWhere((p) => !p.worktree, orElse: () => sorted.first),
  );
  return ProjectGroup(head, [
    for (final p in sorted)
      if (!identical(p, head)) p,
  ]);
}

/// Everything (projects, worktrees, folders) whose name, branch or folder contains every word of [query],
/// case-insensitive, most active first. An empty query matches nothing (the caller shows the groups instead).
List<ProjectInfo> searchProjects(
  List<ProjectInfo> list,
  String query, {
  Set<String> flagged = const {},
}) {
  final words = query.toLowerCase().split(RegExp(r'\s+'))
    ..removeWhere((w) => w.isEmpty);
  if (words.isEmpty) return const [];
  bool matches(ProjectInfo p) {
    final hay = [p.name, p.branch ?? '', p.root].join('\n').toLowerCase();
    return words.every(hay.contains);
  }

  return list.where(matches).toList()
    ..sort((a, b) => _byActivityThenName(a, b, flagged));
}
