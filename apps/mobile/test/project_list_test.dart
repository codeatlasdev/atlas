import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';

void main() {
  // Real hostd output (written by atlas-host's projects::tests). The home screen's
  // "Needs you" reads each project's agents from this payload.
  test('project_list from hostd carries each project agents', () {
    final data = jsonDecode(
      File('test/fixtures/project_list.json').readAsStringSync(),
    ) as Map<String, dynamic>;
    final projects = [
      for (final p in data['projects'] as List)
        ProjectInfo.fromJson(p as Map<String, dynamic>),
    ];
    final shop = projects.firstWhere((p) => p.id == 'a');
    expect((shop.id, shop.name, shop.running), ('a', 'shop', false));
    final (pane, agent) = shop.agents.single;
    expect(pane, 'p1');
    expect((agent.agent, agent.state), ('claude', 'blocked'));
    // What the home groups by: the repository, the branch, worktree or not, project or plain folder.
    final wt = projects.firstWhere((p) => p.id == 'w');
    expect(
      (shop.repo, shop.worktree, shop.isProject, shop.pinned),
      (shop.root, false, true, true),
    );
    expect((wt.repo, wt.branch, wt.worktree), (shop.root, 'feature/x', true));
    expect(projects.firstWhere((p) => p.id == 'f').isProject, isFalse);
  });
}
