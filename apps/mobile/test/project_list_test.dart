import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';

void main() {
  // Real hostd output (written by atlas-host's projects::tests). The home screen's
  // "Needs you" reads each project's agents from this payload.
  test('project_list from hostd carries each project agents', () {
    final data =
        jsonDecode(File('test/fixtures/project_list.json').readAsStringSync())
            as Map<String, dynamic>;
    final projects = [
      for (final p in data['projects'] as List)
        ProjectInfo.fromJson(p as Map<String, dynamic>),
    ];
    final shop = projects.single;
    expect((shop.id, shop.name, shop.running), ('a', 'shop', false));
    final (pane, agent) = shop.agents.single;
    expect(pane, 'p1');
    expect((agent.agent, agent.state), ('claude', 'blocked'));
  });
}
