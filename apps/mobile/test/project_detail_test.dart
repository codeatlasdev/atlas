import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/features/projects/project_metrics.dart';

void main() {
  // Real hostd output (written by atlas-host's projects::tests); a mock here could hide a
  // renamed key, the way a camelCase mock once hid a snake_case field in the desktop.
  final detail = jsonDecode(
    File('test/fixtures/project_detail.json').readAsStringSync(),
  ) as Map<String, dynamic>;

  test('project_detail from hostd parses into the panel model', () {
    final s = ProjectSnapshot.fromDetail(detail);
    expect(s.name, 'shop');
    expect(s.running, isFalse);
    expect(s.errors5m, 1);
    expect(s.services.map((x) => x.name), ['api', 'db']);
    final api = s.services.first;
    // Stopped stack: state defaults to stopped, the port comes from atlas.yaml.
    expect((api.state, api.port, api.up), ('stopped', 3000, false));
    expect(api.lastLine, 'listening on 3000');
    expect(s.services[1].dependsOn, ['api']);
    expect(s.actions.single.label, 'Migrate');
    expect(s.actions.single.confirm, 'sure?');
    // Agents come from hostd's AgentHub serialization, one per terminal in the project.
    final (pane, agent) = s.agents.single;
    expect(pane, 'p-in');
    expect(
      (agent.state, agent.prompt, agent.lastMessage),
      ('done', 'diga oi', 'oi'),
    );
  });

  test('live status wins over config and feeds the KPIs and history', () {
    final live = {
      ...detail,
      'status': {
        'running': true,
        'pid': 42,
        'services': [
          {
            'name': 'api',
            'state': 'running',
            'port': 3001,
            'mem_mb': 100.5,
            'cpu': 3.0,
            'restarts': 2,
            'uptime_s': 3700,
          },
          {
            'name': 'db',
            'state': 'failed',
            'mem_mb': 0,
            'cpu': 0,
            'restarts': 1,
          },
        ],
      },
    };
    final s = ProjectSnapshot.fromDetail(live);
    expect(s.services.first.port, 3001);
    expect((s.upCount, s.restarts, s.memMb, s.uptimeS), (1, 3, 100.5, 3700));
    final h = ProjectHistory(capacity: 2)
      ..add(s)
      ..add(s)
      ..add(s);
    expect(h.mem['api'], [
      100.5,
      100.5,
    ], reason: 'ring keeps only `capacity` samples');
    expect(h.states['db'], ['failed', 'failed']);
  });
}
