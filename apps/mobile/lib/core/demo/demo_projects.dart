/// The projects of the demo computer: the orbit-shop project with its services, logs and
/// checkouts, and two quiet ones, in hostd's `project_*` and `worktree_*` shapes.
library;

import 'package:atlas_mobile/core/demo/demo_agents.dart';
import 'package:atlas_mobile/core/demo/demo_bus.dart';

class DemoService {
  DemoService(
    this.name,
    this.state,
    this.port,
    this.mem,
    this.cpu,
    this.deps,
    this.line,
  );
  final String name;
  String state;
  final int? port;
  final double mem;
  final double cpu;
  final List<String> deps;
  String line;
}

class DemoProjects {
  final DemoBus bus;
  final DemoAgents agents;
  DemoProjects(this.bus, this.agents);

  final _started = DateTime.now();

  final _services = [
    DemoService('api', 'running', 4100, 182, 6, [
      'migrate',
    ], 'GET /orders 200 12ms'),
    DemoService('web', 'running', 4200, 412, 11, [
      'api',
    ], 'ready on http://localhost:4200'),
    DemoService(
      'worker',
      'running',
      null,
      96,
      2,
      [],
      'processed batch 41 (128 jobs)',
    ),
    DemoService(
      'payments',
      'failed',
      null,
      0,
      0,
      [],
      'ERROR ledger refused connection',
    ),
  ];

  /// The demo project's checkouts, like `worktree_list` answers (the dirty one is kept on removal).
  final _worktrees = <Map<String, Object?>>[
    {
      'id': 'demo-shop',
      'path': '/Users/demo/Projetos/orbit-shop',
      'name': 'orbit-shop',
      'branch': 'main',
      'main': true,
      'dirty': false,
      'exists': true,
      'portOffset': 0,
    },
    {
      'id': 'demo-wt-fix',
      'path': '/Users/demo/.atlas/worktrees/orbit-shop/fix-checkout',
      'name': 'fix-checkout',
      'branch': 'fix-checkout',
      'main': false,
      'dirty': true,
      'exists': true,
      'portOffset': 100,
    },
  ];

  final _others = const [
    {
      'id': 'demo-docs',
      'root': '/Users/demo/Projetos/docs',
      'name': 'docs',
      'running': true,
      'services': [
        {
          'name': 'site',
          'state': 'running',
          'port': 3000,
          'mem_mb': 140.0,
          'cpu': 1.0,
        },
      ],
      'agents': <Object>[],
    },
    {
      'id': 'demo-landing',
      'root': '/Users/demo/Projetos/landing',
      'name': 'landing',
      'running': false,
      'services': <Object>[],
      'agents': <Object>[],
    },
  ];

  bool hasWorktree(String id) => _worktrees.any((w) => w['id'] == id);

  Map<String, Object?> _createWorktree(String? name) {
    final slug = (name ?? '')
        .toLowerCase()
        .replaceAll(RegExp(r'[^a-z0-9]+'), '-')
        .replaceAll(RegExp(r'^-+|-+$'), '');
    final n = slug.isEmpty ? 'coral' : slug;
    final offset = 100 * (_worktrees.length);
    final w = <String, Object?>{
      'id': 'demo-wt-$n',
      'path': '/Users/demo/.atlas/worktrees/orbit-shop/$n',
      'name': n,
      'branch': n,
      'main': false,
      'dirty': false,
      'exists': true,
      'portOffset': offset,
    };
    _worktrees.add(w);
    return {
      'id': w['id'],
      'path': w['path'],
      'branch': n,
      'base': 'origin/main',
      'portOffset': offset,
      'repoId': 'demo-shop',
      'name': n,
    };
  }

  Map<String, Object?> _service(String? name, String op) {
    final s = _services.where((x) => x.name == name).firstOrNull;
    if (s == null) return {'text': 'unknown service'};
    s.state = op == 'stop' ? 'stopped' : 'starting';
    if (op != 'stop') {
      bus.later(const Duration(seconds: 2), () {
        // payments fails on purpose: it needs a ledger the demo does not have.
        s.state = s.name == 'payments' ? 'failed' : 'running';
      });
    }
    return {'text': '${s.name}: $op requested'};
  }

  /// Small changes every few seconds so charts and timelines move.
  void jitter() {
    bus.seq++;
    _services[0].line =
        'GET /orders/${4100 + bus.seq} 200 ${8 + bus.seq % 7}ms';
    _services[2].line =
        'processed batch ${41 + bus.seq} (${120 + bus.seq % 16} jobs)';
  }

  double _wave(double base, int k) =>
      base * (1 + .08 * ((bus.seq + k) % 5 - 2) / 2);

  int get _uptime => DateTime.now().difference(_started).inSeconds + 4800;

  List<Map<String, Object?>> _agentsJson() => [
    for (final a in agents.all) {'pane_key': a.pane, 'status': a.status()},
  ];

  Map<String, Object?> _liveService(DemoService s) => {
    'name': s.name,
    'state': s.state,
    'port': s.port,
    'mem_mb': s.state == 'running' ? _wave(s.mem, s.name.length) : 0,
    'cpu': s.state == 'running' ? _wave(s.cpu, s.name.length + 1) : 0,
    'restarts': s.name == 'payments' ? 3 : 0,
    'uptime_s': s.state == 'running' ? _uptime : null,
  };

  Map<String, Object?> _project() => {
    'id': 'demo-shop',
    'root': '/Users/demo/Projetos/orbit-shop',
    'name': 'orbit-shop',
    'running': true,
    'services': [for (final s in _services) _liveService(s)],
    'agents': _agentsJson(),
  };

  Map<String, Object?> _detail() => {
    'id': 'demo-shop',
    'name': 'orbit-shop',
    'root': '/Users/demo/Projetos/orbit-shop',
    'errors_5m': 2,
    'status': {
      'running': true,
      'pid': 48211,
      'setup': {'state': 'ok', 'command': 'bun install'},
      'services': [for (final s in _services) _liveService(s)],
    },
    'config': {
      'services': [
        for (final s in _services)
          {
            'name': s.name,
            'port': s.port,
            'depends_on': [
              for (final d in s.deps) {'service': d},
            ],
          },
      ],
      'actions': [
        {
          'key': 'x',
          'label': 'Seed demo data',
          'confirm': 'Insert 3 batches of fake orders?',
        },
        {'key': 'm', 'label': 'Run migrations', 'confirm': null},
      ],
    },
    'last_lines': {for (final s in _services) s.name: s.line},
    'agents': _agentsJson(),
  };

  Map<String, Object?> _detailOf(String id) {
    final p = _others.firstWhere(
      (p) => p['id'] == id,
      orElse: () => _others.last,
    );
    final services = (p['services'] as List).cast<Map<String, Object?>>();
    return {
      'id': p['id'],
      'name': p['name'],
      'root': p['root'],
      'errors_5m': 0,
      'status': {'running': p['running'], 'services': services},
      'config': {
        'services': [
          for (final s in services)
            {'name': s['name'], 'port': s['port'], 'depends_on': <Object>[]},
        ],
        'actions': <Object>[],
      },
      'last_lines': <String, String>{},
      'agents': <Object>[],
    };
  }

  String _logs(String? service) {
    final now = DateTime.now();
    String t(int s) {
      final d = now.subtract(Duration(seconds: s));
      return '${d.hour.toString().padLeft(2, '0')}:${d.minute.toString().padLeft(2, '0')}:${d.second.toString().padLeft(2, '0')}';
    }

    final all = [
      for (final s in _services.where(
        (s) => service == null || s.name == service,
      )) ...[
        '[${s.name}:1] ${t(90)} [stdout] starting ${s.name}',
        if (s.name == 'payments') ...[
          '[payments:2] ${t(60)} [stdout] connecting to ledger...',
          '[payments:3] ${t(59)} [stderr] ERROR ledger refused connection',
        ] else
          '[${s.name}:2] ${t(5)} [stdout] ${s.line}',
      ],
    ];
    return all.join('\n');
  }

  /// The reply to `cmd`, or null when it is not one of this part's commands.
  DemoReply? handle(String cmd, Map<String, dynamic> args) {
    Object? data;
    String? error;
    switch (cmd) {
      case 'project_list':
        data = {
          'projects': [_project(), ..._others],
        };
      case 'project_detail':
        data = args['id'] == 'demo-shop'
            ? _detail()
            : {..._detailOf(args['id'] as String)};
      case 'pty_open':
        // Same rules as hostd's `pty_open`: ids of the registry only (a path is never a project), and a worktree
        // must be one of THIS project's. The terminal is a plain shell: unlock to type, nothing runs.
        final project = args['project'] as String?;
        final wt = args['worktree'] as String?;
        if (args['crewmate'] != null) {
          error = 'crewmates live on a crew server';
        } else if (project == null || project.isEmpty) {
          error = 'name a project to open a terminal in';
        } else if (project != 'demo-shop' &&
            !_others.any((p) => p['id'] == project)) {
          error = 'unknown project $project';
        } else if (wt != null &&
            (project != 'demo-shop' || !_worktrees.any((w) => w['id'] == wt))) {
          error = 'unknown worktree $wt in this project';
        } else {
          bus.seq++;
          data = {'pty': 'pty-shell-${bus.seq}'};
        }
      case 'worktree_list':
        final p = args['project'] as String?;
        final other = _others.where((o) => o['id'] == p).firstOrNull;
        data = other == null
            ? {'repoId': 'demo-shop', 'worktrees': _worktrees}
            : {
                // A project with no linked worktrees lists only its own folder, main first, like hostd.
                'repoId': p,
                'worktrees': [
                  {
                    'id': p,
                    'path': other['root'],
                    'name': other['name'],
                    'branch': 'main',
                    'main': true,
                    'dirty': false,
                    'exists': true,
                    'portOffset': 0,
                  },
                ],
              };
      case 'worktree_create':
        final made = _createWorktree(args['name'] as String?);
        data = {'worktree': made, 'setup': null, 'seed': null};
      case 'worktree_remove':
        final w = _worktrees.where((w) => w['id'] == args['id']).firstOrNull;
        if (w == null) {
          error = 'unknown worktree';
        } else if (w['main'] == true) {
          error = 'the main checkout cannot be deleted';
        } else if (w['dirty'] == true) {
          data = {'kept': 'dirty'};
        } else {
          _worktrees.remove(w);
          data = {'removed': true, 'branch': w['branch']};
        }
      case 'project_service':
        data = _service(
          args['service'] as String?,
          args['op'] as String? ?? '',
        );
      case 'project_action':
        data = {'text': 'Seeded 3 batches of demo orders.'};
      case 'project_logs':
        data = {'text': _logs(args['service'] as String?)};
      default:
        return null;
    }
    return (data: data, error: error);
  }
}
