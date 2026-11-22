/// The agents of the demo computer: their conversations, subagents, approval and answers,
/// in hostd's `agent_*` shapes.
library;

import 'package:atlas_mobile/core/demo/demo_agent.dart';
import 'package:atlas_mobile/core/demo/demo_agent_seed.dart';
import 'package:atlas_mobile/core/demo/demo_bus.dart';

class DemoAgents {
  final DemoBus bus;
  DemoAgents(this.bus);

  late final _agents = seedDemoAgents();

  Iterable<DemoAgent> get all => _agents.values;

  DemoAgent? operator [](String? pane) => _agents[pane];

  /// The pending approval of `demo-claude`, until answered.
  String? _approvalReq = 'demo-1';

  void _pushStatus(DemoAgent a) => bus.emit({
    'ev': 'agent_status',
    'pane_key': a.pane,
    'status': a.status(),
  });

  String? _send(String? pane, String text, {bool steer = false}) {
    final a = _agents[pane];
    if (a == null) return 'unknown agent pane';
    final canSteer = steer && a.agent == 'omp' && a.state == 'working';
    if (a.state != 'idle' && a.state != 'done' && !canSteer) {
      return 'agent is working';
    }
    a.history.add({'role': 'user', 'text': text});
    a.state = 'working';
    a.startedAt = a.updatedAt = demoNow();
    _pushStatus(a);
    bus.later(const Duration(milliseconds: 2600), () {
      a.history.add({
        'role': 'assistant',
        'text':
            'This is a demo, so nothing ran. On your own computer, ${a.agent} would answer '
            'here, live, and you could keep the conversation going from your phone.',
      });
      a.state = 'done';
      a.updatedAt = demoNow();
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
            'updated_at': demoNow(),
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
    a.updatedAt = demoNow();
    _pushStatus(a);
    return null;
  }

  /// A new agent in a demo project: it takes the first prompt and answers like the others.
  Map<String, Object?> launch(String agent, String? prompt) {
    final pane = 'demo-new-${_agents.length}';
    final a = DemoAgent(pane, agent, 'idle', []);
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

  String? approve(String? pane, String? req, bool allow) {
    final a = _agents[pane];
    if (a == null || req == null || req != _approvalReq) {
      return 'no such pending request';
    }
    _approvalReq = null;
    bus.emit({'ev': 'agent_approval_done', 'req': req, 'pane_key': pane});
    a.state = 'working';
    a.startedAt = a.updatedAt = demoNow();
    _pushStatus(a);
    bus.later(const Duration(milliseconds: 2400), () {
      a.history.add({
        'role': 'assistant',
        'text': allow
            ? 'Migration 0042 applied. orders.created_at is indexed; the slow query dropped from 840 ms to 9 ms.'
            : 'Understood, I did not run the migration. It is in migrations/0042 when you want it.',
      });
      a.state = 'done';
      a.updatedAt = demoNow();
      _pushStatus(a);
    });
    return null;
  }

  /// The reply to `cmd`, or null when it is not one of this part's commands.
  DemoReply? handle(String cmd, Map<String, dynamic> args) {
    Object? data;
    String? error;
    switch (cmd) {
      case 'agent_status_all':
        data = {
          'panes': {for (final a in _agents.values) a.pane: a.status()},
          'hook_port': 0,
          'pending_approvals': [if (_approvalReq != null) _approvalEvent()],
        };
      case 'agent_history':
        data = _history(args['pane'] as String?, args['sub'] as String?);
      case 'agent_tree':
        final a = _agents[args['pane']];
        data = a == null ? {'nodes': <Object>[]} : {'nodes': demoAgentTree(a)};
      case 'agent_interrupt':
        error = _interrupt(args['pane'] as String?);
        data = error == null ? <String, Object?>{} : null;
      case 'agent_send':
        error = _send(
          args['pane'] as String?,
          args['text'] as String? ?? '',
          steer: args['steer'] == true,
        );
        data = error == null ? <String, Object?>{} : null;
      default:
        return null;
    }
    return DemoReply(data: data, error: error);
  }
}
