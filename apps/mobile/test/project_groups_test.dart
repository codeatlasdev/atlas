import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/home/project_groups.dart';

// The home used to list the desktop's registry flat: every worktree as a project and folders opened once
// (`~/Downloads`, `~/.omp`) among them (screenshots 2026-11-20). These run on hostd's real `project_list`
// (written by atlas-host's projects::tests) and on registry shapes seen on a real computer.

List<ProjectInfo> _fixture() {
  final data = jsonDecode(
    File('test/fixtures/project_list.json').readAsStringSync(),
  ) as Map<String, dynamic>;
  return [
    for (final p in data['projects'] as List)
      ProjectInfo.fromJson(p as Map<String, dynamic>),
  ];
}

ProjectInfo _p(
  String id, {
  String? repo,
  String? branch,
  bool worktree = false,
  bool running = false,
  bool folder = false,
  bool pinned = false,
}) => ProjectInfo(
  id: id,
  root: '/u/$id',
  name: id,
  running: running,
  services: const [],
  repo: repo,
  branch: branch,
  worktree: worktree,
  isProject: !folder,
  pinned: pinned,
);

void main() {
  test(
    'hostd output: a worktree hangs under its project, a folder is set apart',
    () {
      final home = groupProjects(_fixture());
      final shop = home.groups.single;
      expect(shop.head.name, 'shop');
      expect([for (final w in shop.worktrees) w.branch], ['feature/x']);
      expect([for (final f in home.folders) f.name], ['Downloads']);
    },
  );

  test('many worktrees of one repository make one row, not one each', () {
    final list = [
      _p('atlas', repo: '/u/atlas', branch: 'main'),
      for (var i = 0; i < 40; i++)
        _p('atlas-wt-$i', repo: '/u/atlas', branch: 'feat/$i', worktree: true),
      _p('ac', repo: '/u/ac', branch: 'main'),
    ];
    final home = groupProjects(list);
    expect([for (final g in home.groups) g.head.name], ['ac', 'atlas']);
    expect(home.groups[1].worktrees, hasLength(40));
  });

  test(
    'a running or flagged project comes first, then pinned, then by name',
    () {
      final home = groupProjects(
        [
          _p('zelvo', repo: '/u/zelvo'),
          _p('beta', repo: '/u/beta', pinned: true),
          _p('wi', repo: '/u/wi'),
          _p('ac', repo: '/u/ac'),
          _p('atlas', repo: '/u/atlas'),
          // Its worktree runs: the whole repository rises.
          _p('atlas-wt', repo: '/u/atlas', worktree: true, running: true),
        ],
        flagged: {'wi'},
      );
      expect(
        [for (final g in home.groups) g.head.name],
        ['wi', 'atlas', 'beta', 'ac', 'zelvo'],
      );
    },
  );

  test('without its main checkout registered, the most active worktree heads the repository', () {
    final home = groupProjects([
      _p('wt-a', repo: '/u/r', branch: 'a', worktree: true),
      _p('wt-b', repo: '/u/r', branch: 'b', worktree: true, running: true),
    ]);
    expect(home.groups.single.head.id, 'wt-b');
    expect(home.groups.single.worktrees.single.id, 'wt-a');
  });

  test('an older hostd (no repo, no kind): every entry is its own project', () {
    final home = groupProjects([_p('a'), _p('b')]);
    expect(home.groups, hasLength(2));
    expect(home.folders, isEmpty);
  });

  test('search finds worktrees by branch and folders by name, every word must match', () {
    final list = [
      _p('atlas', repo: '/u/atlas', branch: 'main'),
      _p(
        'atlas-wt-1',
        repo: '/u/atlas',
        branch: 'feat/yank-list',
        worktree: true,
      ),
      _p('Downloads', folder: true),
      _p('ac', repo: '/u/ac'),
    ];
    expect(
      [for (final p in searchProjects(list, 'yank')) p.id],
      ['atlas-wt-1'],
    );
    expect([for (final p in searchProjects(list, 'DOWN')) p.id], ['Downloads']);
    expect(
      [for (final p in searchProjects(list, 'atlas feat')) p.id],
      ['atlas-wt-1'],
    );
    expect(searchProjects(list, '   '), isEmpty);
  });
}
