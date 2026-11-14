/// The crew server the demo computer plays: two crewmates (one working, one waiting on an
/// approval) with a seeded task history, in the shape `crewmate_list` / `crewmate_tasks` return.
library;

int demoNowSeconds() => DateTime.now().millisecondsSinceEpoch ~/ 1000;

class DemoCrewmate {
  DemoCrewmate(this.id, this.name, this.agent, this.role);
  final String id;
  final String name;
  final String agent;
  final String role;
  String state = 'idle'; // idle | working | needs_you
  final List<Map<String, dynamic>> tasks = [];
  int queued = 0;
  Map<String, dynamic>? pendingApproval;
  Map<String, dynamic>? lastEntry;
  String? memory;

  Map<String, Object?> toListEntry() => {
    'id': id,
    'name': name,
    'role': role,
    'agent': agent,
    'approval': 'ask',
    'state': state,
    'running_task': null,
    'queued': queued,
    'last': lastEntry,
    'pending_approval': pendingApproval,
  };
}

/// Reviewer is working, Nightly needs you.
Map<String, DemoCrewmate> seedDemoCrew() {
  final crew = <String, DemoCrewmate>{};
  crew['crewmate-1'] = DemoCrewmate(
    'crewmate-1',
    'Reviewer',
    'claude',
    'Reviews PRs in this project',
  );
  crew['crewmate-2'] = DemoCrewmate(
    'crewmate-2',
    'Nightly',
    'codex',
    'Runs nightly checks and tests',
  );
  // Set Nightly to needs_you for demo
  crew['crewmate-2']!.state = 'needs_you';
  crew['crewmate-2']!.pendingApproval = {
    'req': 'crewmate-approval-1',
    'tool_name': 'Bash',
    'tool_input': {'command': 'rm -rf dist'},
    'pane_key': 'crew:crewmate-2:task-n2',
  };
  crew['crewmate-2']!.lastEntry = {
    'text': 'Nightly check found issues',
    'state': 'needs_you',
    'ts': demoNowSeconds(),
  };
  // Seed Nightly task history (newest first in list)
  final n2ts = demoNowSeconds() - 55;
  crew['crewmate-2']!.tasks.addAll([
    {
      'id': 'task-n2',
      'ts': n2ts,
      'source': 'routine',
      'prompt': 'Run nightly checks and clean up dist/',
      'state': 'running',
      'started_at': n2ts,
      'finished_at': null,
      'result': null,
      'error': null,
      'steps': [
        {'ts': n2ts + 3, 'text': 'Bash: bun test'},
        {'ts': n2ts + 9, 'text': 'Bash: bun run lint'},
        {'ts': n2ts + 18, 'text': 'Read dist/ directory'},
      ],
    },
    {
      'id': 'task-n1',
      'ts': n2ts - 3600,
      'source': 'routine',
      'prompt': 'Check for dependency updates',
      'state': 'done',
      'started_at': n2ts - 3600,
      'finished_at': n2ts - 3540,
      'result': 'All dependencies up to date. bun.lockb unchanged.',
      'error': null,
      'steps': [],
    },
  ]);
  // Set Reviewer to working
  crew['crewmate-1']!.state = 'working';
  crew['crewmate-1']!.lastEntry = {
    'text': 'Reviewing PR #118',
    'state': 'working',
    'ts': demoNowSeconds(),
  };
  // Seed Reviewer task history
  final r3ts = demoNowSeconds() - 90;
  crew['crewmate-1']!.tasks.addAll([
    {
      'id': 'task-r3',
      'ts': r3ts,
      'source': 'mobile',
      'prompt': 'Review PR #118 — auth middleware refactor',
      'state': 'running',
      'started_at': r3ts,
      'finished_at': null,
      'result': null,
      'error': null,
      'steps': [
        {'ts': r3ts + 2, 'text': 'Read src/middleware/auth.ts'},
        {'ts': r3ts + 8, 'text': 'Read src/middleware/auth.test.ts'},
        {'ts': r3ts + 15, 'text': 'Read src/routes/user.ts'},
        {'ts': r3ts + 28, 'text': 'Bash: bun test --filter auth'},
      ],
    },
    {
      'id': 'task-r2',
      'ts': r3ts - 180,
      'source': 'desktop',
      'prompt': 'Why is the payments service failing?',
      'state': 'done',
      'started_at': r3ts - 180,
      'finished_at': r3ts - 120,
      'result': 'payments exits with code 2: connects to a ledger service not in this project. Add LEDGER_URL to .env or stub with LEDGER_MOCK=1.',
      'error': null,
      'steps': [],
    },
    {
      'id': 'task-r1',
      'ts': r3ts - 720,
      'source': 'routine',
      'prompt': 'Review open PRs and leave comments',
      'state': 'done',
      'started_at': r3ts - 720,
      'finished_at': r3ts - 660,
      'result': 'Found 2 issues in PR #115: missing null check on line 42 and an unused import. Left inline comments.',
      'error': null,
      'steps': [],
    },
  ]);
  return crew;
}
