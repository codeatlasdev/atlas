/// What the demo computer's agents start with: a blocked Claude with an approval to answer,
/// and an omp that has finished, with a subagent tree.
library;

import 'package:atlas_mobile/core/demo/demo_agent.dart';

Map<String, DemoAgent> seedDemoAgents() => {
  'demo-claude': DemoAgent(
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
  'demo-omp': DemoAgent(
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
        'report':
            'payments/atlas.yaml points at localhost:4300; nothing starts it.',
      },
    ],
  ),
};

/// `agent_tree` of `demo-omp`: a scout that spawned a deeper one.
List<Map<String, Object?>> demoAgentTree(DemoAgent a) {
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
                  {'content': 'Find the ledger config', 'status': 'completed'},
                  {'content': 'Report the ledger URL', 'status': 'in_progress'},
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
