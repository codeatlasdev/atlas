/// The crew server the demo computer plays: two crewmates (one working, one waiting on an
/// approval) with a seeded task history, in the shape `crewmate_list` / `crewmate_tasks` return.
library;

int demoNowSeconds() => DateTime.now().millisecondsSinceEpoch ~/ 1000;

/// Whether the demo accepts [cron] the way hostd's parser does for the cases a phone can send: five
/// fields, each number inside its field's range (`*`, `*/n`, `a-b`, `a,b` built from such numbers).
bool demoCronOk(String cron) {
  final parts = cron.trim().split(RegExp(r'\s+'));
  if (parts.length != 5) return false;
  const max = [59, 23, 31, 12, 7];
  const min = [0, 0, 1, 1, 0];
  for (var i = 0; i < 5; i++) {
    for (final piece in parts[i].split(',')) {
      final body = piece.split('/').first;
      if (body == '*' || body == '?') continue;
      for (final n in body.split('-')) {
        final v = int.tryParse(n);
        if (v == null || v < min[i] || v > max[i]) return false;
      }
    }
  }
  return true;
}

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

  /// The task parked on a card (`prove_ui`), listed as `waiting_task` for the Captain's call.
  Map<String, dynamic>? waiting;
  Map<String, dynamic>? lastEntry;
  String? memory;

  /// Cron routines, in the shape `crewmate_list.routines` returns and `crewmate_routine_set` replaces.
  List<Map<String, dynamic>> routines = [];

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
    if (waiting != null)
      'waiting_task': {
        'id': waiting!['id'],
        'action': waiting!['action'],
        'result': waiting!['result'],
        'attachments': [
          for (final a in (waiting!['attachments'] as List? ?? const []))
            if ((a as Map)['source'] == 'agent') a,
        ],
      },
    'triggers': <Object>[],
    'routines': routines,
  };
}

/// The crew's operations data the demo answers: the audit log (`crewmate_audit` / `crew_audit`), token
/// usage (`crewmate_usage`), engine limits and the crew settings. The audit grows with what the app
/// does on the demo (a routine saved, the crew paused), like hostd's.
class DemoCrewOps {
  DemoCrewOps(this._crewmates) {
    final now = demoNowSeconds();
    // Reviewer's seeded history: a task, a tool call, an approval, a setting.
    _log(now - 600, 'crewmate-1', 'task_queued', {
      'source': 'desktop',
      'prompt': 'Why is the payments service failing?',
      'task_id': 'task-r2',
    });
    _log(now - 595, 'crewmate-1', 'task_started', {
      'source': 'desktop',
      'prompt': 'Why is the payments service failing?',
      'task_id': 'task-r2',
    });
    _log(now - 580, 'crewmate-1', 'tool_call', {
      'tool_name': 'Bash',
      'tool_input': {'command': 'bun test --filter payments'},
      'task_id': 'task-r2',
    });
    _log(now - 570, 'crewmate-1', 'approval_asked', {
      'tool_name': 'Bash',
      'tool_input': {'command': 'cat .env'},
      'req': 'demo-req-1',
      'task_id': 'task-r2',
    });
    _log(now - 560, 'crewmate-1', 'approval_decision', {
      'tool_name': 'Bash',
      'decision': 'deny',
      'by': 'phone:demo0000phone',
      'req': 'demo-req-1',
      'task_id': 'task-r2',
    });
    _log(now - 540, 'crewmate-1', 'task_finished', {
      'state': 'done',
      'source': 'desktop',
      'error': null,
      'task_id': 'task-r2',
    });
    _log(now - 500, 'crewmate-1', 'config_changed', {
      'field': 'approval',
      'from': 'auto',
      'to': 'ask',
    });
    _crewLog(now - 900, 'crewmate_created', {
      'id': 'crewmate-1',
      'name': 'Reviewer',
      'agent': 'claude',
    });
    _crewLog(now - 880, 'crewmate_created', {
      'id': 'crewmate-2',
      'name': 'Nightly',
      'agent': 'codex',
    });
  }

  final Map<String, DemoCrewmate> _crewmates;
  int _seq = 0;
  final _entries = <Map<String, dynamic>>[];

  /// Crew-wide settings (`crew_settings_get`): short replies on, no model chosen.
  bool shortReplies = true;
  String? shortRepliesModel;

  void _log(int ts, String? crewmate, String kind, Map<String, dynamic> extra) {
    _entries.add({
      'seq': ++_seq,
      'ts': ts,
      'kind': kind,
      'crewmate_id': ?crewmate,
      ...extra,
    });
  }

  void _crewLog(int ts, String kind, Map<String, dynamic> extra) =>
      _log(ts, null, kind, extra);

  /// Records a change the app made on the demo crew.
  void record(String? crewmate, String kind, Map<String, dynamic> extra) =>
      _log(demoNowSeconds(), crewmate, kind, extra);

  /// Newest first, like hostd; `kind` and `since` filter, `limit` caps (default 100, as hostd).
  Map<String, Object?> audit(
    String? crewmate, {
    int? since,
    int limit = 100,
    String? kind,
  }) {
    final rows = _entries
        .where(
          (e) => crewmate == null
              ? e['crewmate_id'] == null
              : e['crewmate_id'] == crewmate,
        )
        .where((e) => since == null || (e['ts'] as int) >= since)
        .where((e) => kind == null || e['kind'] == kind)
        .toList()
        .reversed
        .take(limit)
        .toList();
    return {'entries': rows};
  }

  /// Usage records of one crewmate (or all): a few days of the demo crew's turns.
  Map<String, Object?> usage(String? crewmate, {int since = 0}) {
    final now = demoNowSeconds();
    final out = <Map<String, Object?>>[];
    for (final id in _crewmates.keys) {
      if (crewmate != null && crewmate != id) continue;
      for (var d = 0; d < 6; d++) {
        out.add({
          'ts': now - d * 86400 - 3600,
          'engine': id == 'crewmate-1' ? 'claude' : 'codex',
          'model': id == 'crewmate-1' ? 'claude-sonnet-4' : 'gpt-5',
          'input_tokens': 1800 + d * 450,
          'output_tokens': 320 + d * 60,
          'cost_usd': id == 'crewmate-1' ? 0.02 + d * 0.004 : null,
          'duration_ms': 3000 + d * 500,
          'crewmate_id': id,
        });
      }
    }
    return {
      'records': [
        for (final r in out)
          if ((r['ts'] as int) >= since) r,
      ],
    };
  }
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
