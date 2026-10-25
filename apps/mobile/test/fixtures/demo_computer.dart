import 'dart:async';
import 'dart:convert';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';

/// The demo computer's identity. Never dialled: [demoTransport] answers for it.
const demoDevice = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

bool isDemo(MacDevice d) => d.nodeId == demoDevice.nodeId;

/// A computer that lives in the app: it answers the same protocol as `atlas hostd`
/// (replies by id, pushes with `ev`), so every screen runs its real code against it.
/// Used by tests that need a self-contained Transport without a real host.
Transport demoTransport({List<Map<String, String>> integrations = const []}) =>
    _DemoComputer(integrations);

int _now() => DateTime.now().millisecondsSinceEpoch;

class _Agent {
  _Agent(
    this.pane,
    this.agent,
    this.state,
    this.history, {
    this.title,
    this.steps = const [],
    this.subagents = const [],
  });
  final String pane;
  final String agent;
  String state;
  final List<Map<String, String>> history;
  String? title;

  /// Tool calls and subagent calls shown between the first prompt and the answer, in
  /// hostd's `agent_history` item shape.
  final List<Map<String, Object?>> steps;
  final List<Map<String, Object?>> subagents;
  int updatedAt = _now();
  int startedAt = _now();

  /// `agent_history`'s `items`: the conversation plus its tool steps. The steps sit
  /// before the last assistant message, like a real turn.
  List<Map<String, Object?>> items() {
    final msgs = [
      for (final m in history) {'kind': m['role'], 'text': m['text']},
    ];
    if (steps.isEmpty || msgs.isEmpty) return msgs;
    final at = msgs.lastIndexWhere((m) => m['kind'] == 'user') + 1;
    return [
      ...msgs.take(at),
      {'kind': 'thinking', 'text': 'Look at the code before changing it.'},
      ...steps,
      ...msgs.skip(at),
    ];
  }

  Map<String, Object?> status() => {
    'state': state,
    'agent': agent,
    'session_id': null,
    'session_file': null,
    'model': switch (agent) {
      'claude' => 'claude-sonnet',
      'omp' => 'gpt-5.5',
      _ => null,
    },
    'prompt': history.lastWhere(
      (m) => m['role'] == 'user',
      orElse: () => const {},
    )['text'],
    'last_message': state == 'working'
        ? null
        : history.lastWhere(
            (m) => m['role'] == 'assistant',
            orElse: () => const {},
          )['text'],
    'updated_at': updatedAt,
    'state_started_at': startedAt,
    'worktree_id': null,
    'tab_id': null,
    'title': title,
  };
}

class _Service {
  _Service(
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

class _DemoComputer implements Transport {
  /// `agent_integrations` agents, in hostd's shape (`{id, label, state}`).
  final List<Map<String, String>> _integrations;

  _DemoComputer(this._integrations) {
    _tick = Timer.periodic(const Duration(seconds: 3), (_) => _jitter());
  }

  final _lines = StreamController<String>.broadcast();
  late final Timer _tick;
  bool _open = true;
  final _started = DateTime.now();
  int _seq = 0;

  final _services = [
    _Service('api', 'running', 4100, 182, 6, [
      'migrate',
    ], 'GET /orders 200 12ms'),
    _Service('web', 'running', 4200, 412, 11, [
      'api',
    ], 'ready on http://localhost:4200'),
    _Service(
      'worker',
      'running',
      null,
      96,
      2,
      [],
      'processed batch 41 (128 jobs)',
    ),
    _Service(
      'payments',
      'failed',
      null,
      0,
      0,
      [],
      'ERROR ledger refused connection',
    ),
  ];

  late final _agents = {
    'demo-claude': _Agent(
      'demo-claude',
      'claude',
      'blocked',
      [
        {
          'role': 'user',
          'text': 'Add an index on orders.created_at and run the migration',
        },
        {
          'role': 'assistant',
          'text': 'I wrote migration `0042_orders_created_at`. I need your approval to run it.',
        },
      ],
      title: 'Index orders.created_at',
      steps: [
        {
          'kind': 'tool',
          'id': 'c1',
          'name': 'read',
          'intent': 'Reading the schema',
          'detail': 'db/schema.sql',
          'status': 'ok',
          'output': 'Read 214 lines',
        },
        {
          'kind': 'tool',
          'id': 'c2',
          'name': 'write',
          'intent': 'Writing migration 0042',
          'detail': 'db/migrations/0042_orders_created_at.sql',
          'status': 'ok',
          'output': 'CREATE INDEX CONCURRENTLY orders_created_at_idx ON orders (created_at);',
        },
      ],
    ),
    'demo-omp': _Agent(
      'demo-omp',
      'omp',
      'done',
      [
        {'role': 'user', 'text': 'Why is payments failing?'},
        {
          'role': 'assistant',
          'text':
              'payments exits with code 2: its command connects to a ledger service that '
              'is not part of this project, so the connection is refused on start.\n\n'
              '| Service | State |\n|---|---|\n| payments | failed |\n| api | running |',
        },
      ],
      title: 'Why is payments failing?',
      steps: [
        {
          'kind': 'tool',
          'id': 'o1',
          'name': 'bash',
          'intent': 'Reading the payments log',
          'detail': 'atlas logs payments',
          'status': 'ok',
          'output': 'ERROR ledger refused connection',
        },
        {
          'kind': 'tool',
          'id': 'o2',
          'name': 'bash',
          'intent': 'Checking the ledger service',
          'detail': 'curl -sf localhost:4300/health',
          'status': 'error',
          'output': 'curl: (7) Failed to connect to localhost port 4300',
        },
        {
          'kind': 'tool',
          'id': 'o3',
          'name': 'task',
          'status': 'ok',
          'output': 'Spawned 1 agent',
          'agents': [
            {
              'name': 'LedgerScout',
              'agent': 'scout',
              'task': 'Find where the ledger service is configured',
            },
          ],
        },
        {
          'kind': 'result',
          'id': 'LedgerScout',
          'status': 'completed',
          'duration': '41s',
        },
        {
          'kind': 'tool',
          'id': 'o4',
          'name': 'edit',
          'intent': 'Pointing payments at the mock ledger',
          'detail': 'payments/atlas.yaml',
          'status': 'ok',
          'output': 'Updated 1 line',
        },
      ],
      subagents: [
        {
          'name': 'LedgerScout',
          'task': 'Find where the ledger service is configured',
          'state': 'done',
          'report': 'payments/atlas.yaml points at localhost:4300; nothing starts it.',
        },
      ],
    ),
  };

  /// `agent_tree` of `demo-omp`: a scout that spawned a deeper one.
  List<Map<String, Object?>> _tree(_Agent a) {
    final running = a.state == 'working';
    final root = {
      'id': '',
      'parent': null,
      'name': a.title,
      'agent': a.agent,
      'model': 'gpt-5.5',
      'status': switch (a.state) {
        'working' || 'blocked' || 'waiting' => 'running',
        'done' => 'completed',
        _ => 'idle',
      },
      'task': a.history.firstOrNull?['text'],
      'duration_ms': 90000,
      'tokens': 48200,
      'cost': 0.31,
      'tool_count': a.steps.length,
      'now': running ? 'Reading the payments log' : null,
      'report': null,
      'updated_at': a.updatedAt,
      'agent_type': null,
      'thinking': 'high',
      'requests': 6,
      'active_ms': 61000,
      'context_tokens': 292000,
      'context_window': 1000000,
      'recent': <Object>[],
      'todos': a.subagents.isEmpty
          ? null
          : {
              'phases': [
                {
                  'name': 'Investigate',
                  'tasks': [
                    {
                      'content': 'Find the ledger config',
                      'status': 'completed',
                    },
                    {
                      'content': 'Report the ledger URL',
                      'status': 'in_progress',
                    },
                  ],
                },
              ],
              'done': 1,
              'total': 2,
            },
    };
    if (a.subagents.isEmpty) return [root];
    return [
      root,
      {
        'id': 'LedgerScout',
        'parent': '',
        'name': 'LedgerScout',
        'agent': 'scout',
        'model': 'claude-haiku',
        'status': 'completed',
        'task': 'Find where the ledger service is configured',
        'duration_ms': 41000,
        'tokens': 9100,
        'cost': 0.02,
        'tool_count': 4,
        'now': null,
        'report':
            'payments/atlas.yaml points at localhost:4300; nothing starts it.',
        'updated_at': a.updatedAt,
        'agent_type': 'scout',
        'thinking': 'low',
        'requests': 3,
        'active_ms': 38000,
        'context_tokens': 9100,
        'context_window': 200000,
        'recent': [
          {
            'at': a.updatedAt - 5000,
            'tool': 'read',
            'intent': 'Reading payments/atlas.yaml',
            'status': 'ok',
          },
        ],
        'todos': null,
      },
      {
        'id': 'LedgerScout.Deep',
        'parent': 'LedgerScout',
        'name': 'LedgerScout.Deep',
        'agent': 'scout',
        'model': 'claude-haiku',
        'status': 'completed',
        'task': 'Read payments/atlas.yaml and report the ledger URL',
        'duration_ms': 12000,
        'tokens': 2300,
        'cost': 0.004,
        'tool_count': 1,
        'now': null,
        'report': 'ledger url: localhost:4300',
        'updated_at': a.updatedAt,
        'agent_type': 'scout',
        'thinking': 'off',
        'requests': 1,
        'active_ms': 9000,
        'context_tokens': null,
        'context_window': null,
        'recent': <Object>[],
        'todos': null,
      },
    ];
  }

  /// The pending approval of `demo-claude`, until answered.
  String? _approvalReq = 'demo-1';

  @override
  Stream<String> get incomingLines => _lines.stream;

  @override
  Future<void> close() async {
    if (!_open) return;
    _open = false;
    _tick.cancel();
    await _lines.close();
  }

  void _emit(Map<String, Object?> v) {
    if (_open) _lines.add(jsonEncode(v));
  }

  void _later(Duration d, void Function() f) => Timer(d, () {
    if (_open) f();
  });

  void _pushStatus(_Agent a) =>
      _emit({'ev': 'agent_status', 'pane_key': a.pane, 'status': a.status()});

  @override
  Future<void> sendLine(String line) async {
    if (!_open) throw StateError('closed');
    final req = jsonDecode(line) as Map<String, dynamic>;
    final id = req['id'];
    final args = (req['args'] as Map?)?.cast<String, dynamic>() ?? const {};
    Object? data;
    String? error;
    switch (req['cmd']) {
      case 'project_list':
        data = {
          'projects': [_project(), ..._others],
        };
      case 'project_detail':
        data = args['id'] == 'demo-shop'
            ? _detail()
            : {..._detailOf(args['id'] as String)};
      case 'agent_status_all':
        data = {
          'panes': {for (final a in _agents.values) a.pane: a.status()},
          'hook_port': 0,
          'pending_approvals': [if (_approvalReq != null) _approvalEvent()],
        };
      case 'agent_integrations':
        data = {'agents': _integrations};
      case 'agent_history':
        data = _history(args['pane'] as String?, args['sub'] as String?);
      case 'agent_tree':
        final a = _agents[args['pane']];
        data = a == null ? {'nodes': <Object>[]} : {'nodes': _tree(a)};
      case 'agent_interrupt':
        error = _interrupt(args['pane'] as String?);
        data = error == null ? <String, Object?>{} : null;
      case 'agent_launch':
        data = _launch(
          args['agent'] as String? ?? 'omp',
          args['prompt'] as String?,
        );
      case 'agent_send':
        error = _send(
          args['pane'] as String?,
          args['text'] as String? ?? '',
          steer: args['steer'] == true,
        );
        data = error == null ? <String, Object?>{} : null;
      case 'agent_approve':
        error = _approve(
          args['pane'] as String?,
          args['req'] as String?,
          args['allow'] == true,
        );
        data = error == null ? <String, Object?>{} : null;
      case 'project_service':
        data = _service(
          args['service'] as String?,
          args['op'] as String? ?? '',
        );
      case 'project_action':
        data = {'text': 'Seeded 3 batches of demo orders.'};
      case 'project_logs':
        data = {'text': _logs(args['service'] as String?)};
      case 'pty_list':
        data = {
          'ptys': [
            for (final a in _agents.values)
              {
                'pty': 'pty-${a.pane}',
                'cwd': '~/Projetos/orbit-shop',
                'argv': [a.agent],
                'scope': {'pane_key': a.pane},
              },
          ],
        };
      case 'pty_info':
        data = {'pty': args['pty'], ..._size(args['pty'] as String)};
      case 'pty_attach':
        // Same order as hostd (mobile/session.rs): the replay goes out before the
        // reply, so a screen that listens only after attach returns sees nothing.
        _replay(args['pty'] as String);
        data = {'pty': args['pty']};
      case 'pty_write':
        final text = utf8.decode(base64.decode(args['data'] as String? ?? ''));
        _emit({
          'ev': 'pty_output',
          'pty': args['pty'],
          'data': base64.encode(utf8.encode(text.replaceAll('\r', '\r\n'))),
        });
        data = <String, Object?>{};
      default:
        data = <String, Object?>{};
    }
    final reply = error == null
        ? {'id': id, 'ok': true, 'data': data}
        : {'id': id, 'ok': false, 'error': error};
    scheduleMicrotask(() => _emit(reply));
  }

  String? _send(String? pane, String text, {bool steer = false}) {
    final a = _agents[pane];
    if (a == null) return 'unknown agent pane';
    final canSteer = steer && a.agent == 'omp' && a.state == 'working';
    if (a.state != 'idle' && a.state != 'done' && !canSteer) {
      return 'agent is working';
    }
    a.history.add({'role': 'user', 'text': text});
    a.state = 'working';
    a.startedAt = a.updatedAt = _now();
    _pushStatus(a);
    _later(const Duration(milliseconds: 2600), () {
      a.history.add({
        'role': 'assistant',
        'text':
            'This is a demo, so nothing ran. On your own computer, ${a.agent} would answer '
            'here, live, and you could keep the conversation going from your phone.',
      });
      a.state = 'done';
      a.updatedAt = _now();
      _pushStatus(a);
    });
    return null;
  }

  /// `agent_history` in hostd's shape: items, title, and the subagents (or one
  /// subagent's own transcript with `sub`).
  Map<String, Object?> _history(String? pane, String? sub) {
    final a = _agents[pane];
    if (a == null) return {'items': [], 'messages': [], 'truncated': false};
    if (sub != null) {
      final s = a.subagents
          .where((x) => x['name'] == sub || sub.startsWith('${x['name']}.'))
          .firstOrNull;
      final deep = sub.contains('.');
      final items = <Map<String, Object?>>[
        {
          'kind': 'user',
          'text': deep
              ? 'Read payments/atlas.yaml and report the ledger URL'
              : s?['task'] ?? '',
        },
        {
          'kind': 'tool',
          'id': 's1',
          'name': 'grep',
          'intent': 'Searching for z-index rules',
          'detail': 'z-index',
          'status': 'ok',
          'output': 'nav.css:12: z-index: 40\nsheet.css:4: z-index: 50',
        },
        if (!deep)
          {
            'kind': 'result',
            'id': '$sub.Deep',
            'status': 'completed',
            'duration': '12s',
          },
        {
          'kind': 'assistant',
          'text': deep ? 'ledger url: localhost:4300' : s?['report'] ?? '',
        },
      ];
      return {
        'items': items,
        'messages': [
          for (final i in items)
            if (i['kind'] == 'user' || i['kind'] == 'assistant')
              {'role': i['kind'], 'text': i['text']},
        ],
        'title': sub,
        'truncated': false,
      };
    }
    return {
      'items': a.items(),
      'messages': a.history,
      'title': a.title,
      'truncated': false,
      'subagents': [
        for (final s in a.subagents)
          {
            'name': s['name'],
            'state': s['state'],
            'updated_at': _now(),
            'report': s['report'],
          },
      ],
    };
  }

  String? _interrupt(String? pane) {
    final a = _agents[pane];
    if (a == null) return 'unknown agent pane';
    if (a.state != 'working') return 'agent is not working';
    a.state = 'idle';
    a.updatedAt = _now();
    _pushStatus(a);
    return null;
  }

  /// A new agent in a demo project: it takes the first prompt and answers like the others.
  Map<String, Object?> _launch(String agent, String? prompt) {
    final pane = 'demo-new-${_agents.length}';
    final a = _Agent(pane, agent, 'idle', []);
    _agents[pane] = a;
    _pushStatus(a);
    if (prompt != null) _send(pane, prompt);
    return {'pane': pane, 'pty': 'pty-$pane'};
  }

  Map<String, Object?> _approvalEvent() => {
    'ev': 'agent_approval',
    'pane_key': 'demo-claude',
    'agent': 'claude',
    'req': _approvalReq,
    'tool_name': 'Bash',
    'tool_input': {'command': 'bun run db:migrate'},
  };

  String? _approve(String? pane, String? req, bool allow) {
    final a = _agents[pane];
    if (a == null || req == null || req != _approvalReq) {
      return 'no such pending request';
    }
    _approvalReq = null;
    _emit({'ev': 'agent_approval_done', 'req': req, 'pane_key': pane});
    a.state = 'working';
    a.startedAt = a.updatedAt = _now();
    _pushStatus(a);
    _later(const Duration(milliseconds: 2400), () {
      a.history.add({
        'role': 'assistant',
        'text': allow
            ? 'Migration 0042 applied. orders.created_at is indexed; the slow query dropped from 840 ms to 9 ms.'
            : 'Understood, I did not run the migration. It is in migrations/0042 when you want it.',
      });
      a.state = 'done';
      a.updatedAt = _now();
      _pushStatus(a);
    });
    return null;
  }

  Map<String, Object?> _service(String? name, String op) {
    final s = _services.where((x) => x.name == name).firstOrNull;
    if (s == null) return {'text': 'unknown service'};
    s.state = op == 'stop' ? 'stopped' : 'starting';
    if (op != 'stop') {
      _later(const Duration(seconds: 2), () {
        // payments fails on purpose: it needs a ledger the demo does not have.
        s.state = s.name == 'payments' ? 'failed' : 'running';
      });
    }
    return {'text': '${s.name}: $op requested'};
  }

  /// Small changes every few seconds so charts and timelines move.
  void _jitter() {
    _seq++;
    _services[0].line = 'GET /orders/${4100 + _seq} 200 ${8 + _seq % 7}ms';
    _services[2].line =
        'processed batch ${41 + _seq} (${120 + _seq % 16} jobs)';
  }

  /// Claude runs full screen in a wide Mac terminal, like the real one.
  Map<String, int> _size(String pty) => pty == 'pty-demo-claude'
      ? {'cols': 110, 'rows': 34}
      : {'cols': 80, 'rows': 30};

  void _replay(String pty) {
    if (pty == 'pty-demo-claude') return _replayTui(pty);
    final a = _agents[pty.replaceFirst('pty-', '')];
    final buf = StringBuffer()
      ..write(
        '\x1b[38;2;122;162;247m${a?.agent ?? 'shell'}\x1b[0m  ~/Projetos/orbit-shop\r\n\r\n',
      );
    for (final m in a?.history ?? const <Map<String, String>>[]) {
      final who = m['role'] == 'user'
          ? '\x1b[38;2;158;206;106m❯\x1b[0m '
          : '  ';
      buf.write('$who${m['text']!.replaceAll('\n', '\r\n  ')}\r\n\r\n');
    }
    buf.write(
      '\x1b[2m(demo terminal: unlock to type, nothing runs)\x1b[0m\r\n',
    );
    _emit({
      'ev': 'pty_replay',
      'pty': pty,
      'data': base64.encode(utf8.encode(buf.toString())),
    });
  }

  /// A full-screen agent's screen: alternate buffer, laid out for 110 columns.
  void _replayTui(String pty) {
    const w = 110;
    String row(String text, [String sgr = '']) =>
        '\x1b[38;2;65;72;104m│\x1b[0m $sgr${text.padRight(w - 4)}\x1b[0m \x1b[38;2;65;72;104m│\x1b[0m\r\n';
    final buf = StringBuffer('\x1b[?1049h\x1b[H')
      ..write('\x1b[38;2;65;72;104m╭${'─' * (w - 2)}╮\x1b[0m\r\n')
      ..write(row('✻ Claude Code', '\x1b[1;38;2;224;175;104m'))
      ..write(row('~/Projetos/orbit-shop', '\x1b[2m'))
      ..write('\x1b[38;2;65;72;104m╰${'─' * (w - 2)}╯\x1b[0m\r\n\r\n')
      ..write(
        '\x1b[38;2;158;206;106m>\x1b[0m Add an index on orders.created_at and run the migration\r\n\r\n',
      )
      ..write('\x1b[38;2;192;202;245m⏺\x1b[0m Read(db/schema.sql)\r\n')
      ..write('  ⎿  Read 214 lines\r\n\r\n')
      ..write(
        '\x1b[38;2;192;202;245m⏺\x1b[0m Write(db/migrations/0042_orders_created_at.sql)\r\n',
      )
      ..write(
        '  ⎿  \x1b[38;2;158;206;106m+ CREATE INDEX CONCURRENTLY orders_created_at_idx ON orders (created_at);\x1b[0m\r\n\r\n',
      )
      ..write('\x1b[38;2;192;202;245m⏺\x1b[0m Bash(bun run db:migrate)\r\n\r\n')
      ..write('\x1b[38;2;224;175;104m╭${'─' * (w - 2)}╮\x1b[0m\r\n')
      ..write(row('Bash command', '\x1b[1m'))
      ..write(row('  bun run db:migrate'))
      ..write(
        row('  Run the pending migration 0042 against the local database'),
      )
      ..write(row(''))
      ..write(row('Do you want to proceed?'))
      ..write(row('❯ 1. Yes', '\x1b[38;2;122;162;247m'))
      ..write(
        row('  2. Yes, and don\'t ask again for bun run db:migrate commands'),
      )
      ..write(row('  3. No, and tell Claude what to do differently (esc)'))
      ..write('\x1b[38;2;224;175;104m╰${'─' * (w - 2)}╯\x1b[0m\r\n');
    _emit({
      'ev': 'pty_replay',
      'pty': pty,
      'data': base64.encode(utf8.encode(buf.toString())),
    });
  }

  double _wave(double base, int k) =>
      base * (1 + .08 * ((_seq + k) % 5 - 2) / 2);

  int get _uptime => DateTime.now().difference(_started).inSeconds + 4800;

  List<Map<String, Object?>> _agentsJson() => [
    for (final a in _agents.values) {'pane_key': a.pane, 'status': a.status()},
  ];

  Map<String, Object?> _liveService(_Service s) => {
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
}
