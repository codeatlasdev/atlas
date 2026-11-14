/// Tests for board.dart — mirrors apps/desktop/src/lib/crew/board.test.ts cases.
library;

import 'package:flutter_test/flutter_test.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/features/crew/board.dart';

// ── helpers ───────────────────────────────────────────────────────────────────

// Midday on a fixed date so "today" / "yesterday" boundaries are stable.
final _kNow = DateTime.utc(2026, 10, 29, 12, 0, 0).millisecondsSinceEpoch;

/// Local midnight for [ts] (start of that calendar day in local time).
int _localMidnight(int ts) {
  final d = DateTime.fromMillisecondsSinceEpoch(ts);
  return DateTime(d.year, d.month, d.day).millisecondsSinceEpoch;
}

final _kTodayStart = _localMidnight(_kNow);
final _kYesterday = _kTodayStart - 1; // 1 ms before today

CrewmateEntry _makeCrewmate({
  String id = 'crewmate1',
  String name = 'TestCrewmate',
  String state = 'idle',
  CrewmatePendingApproval? pendingApproval,
}) => CrewmateEntry(
  id: id,
  name: name,
  role: '',
  agent: 'omp',
  approval: 'ask',
  state: state,
  pendingApproval: pendingApproval,
);

CrewmateTask _makeTask({
  String id = 'task1',
  int? ts,
  String source = 'trigger',
  String prompt = 'Do something',
  String state = 'queued',
  String? title,
  int? startedAt,
  int? finishedAt,
}) => CrewmateTask(
  id: id,
  ts: ts ?? _kNow - 10000,
  source: source,
  prompt: prompt,
  state: state,
  title: title,
  startedAt: startedAt,
  finishedAt: finishedAt,
);

BoardEntry _entry(CrewmateTask task, {CrewmateEntry? crewmate}) =>
    BoardEntry(hostKey: '', crewmate: crewmate ?? _makeCrewmate(), task: task);

// ── column assignment ─────────────────────────────────────────────────────────

void main() {
  group('boardColumns: column assignment', () {
    test('queued task goes to Queued', () {
      final cols = boardColumns([_entry(_makeTask(state: 'queued'))], _kNow);
      expect(cols[0].id, ColumnId.queued);
      expect(cols[0].cards, hasLength(1));
      expect(cols[1].cards, isEmpty);
      expect(cols[2].cards, isEmpty);
      expect(cols[3].cards, isEmpty);
    });

    test('running task goes to Working', () {
      final cols = boardColumns([_entry(_makeTask(state: 'running'))], _kNow);
      expect(cols[1].id, ColumnId.working);
      expect(cols[1].cards, hasLength(1));
    });

    test(
      'running task whose crewmate has pending_approval goes to Needs you',
      () {
        final crewmate = _makeCrewmate(
          pendingApproval: const CrewmatePendingApproval(
            req: 'r',
            toolName: 'bash',
            toolInput: {},
            paneKey: 'p',
          ),
        );
        final cols = boardColumns([
          _entry(_makeTask(state: 'running'), crewmate: crewmate),
        ], _kNow);
        expect(cols[2].id, ColumnId.needsYou);
        expect(cols[2].cards, hasLength(1));
        expect(cols[1].cards, isEmpty);
      },
    );

    test('waiting_action task goes to Needs you', () {
      final cols = boardColumns([
        _entry(_makeTask(state: 'waiting_action')),
      ], _kNow);
      expect(cols[2].id, ColumnId.needsYou);
      expect(cols[2].cards, hasLength(1));
    });

    test('done task finished today goes to Done', () {
      final cols = boardColumns([
        _entry(_makeTask(state: 'done', finishedAt: _kNow - 3600000)),
      ], _kNow);
      expect(cols[3].id, ColumnId.done);
      expect(cols[3].cards, hasLength(1));
    });

    test('done task finished before today is excluded', () {
      final cols = boardColumns([
        _entry(_makeTask(state: 'done', finishedAt: _kYesterday)),
      ], _kNow);
      expect(cols[3].cards, isEmpty);
    });

    test('done task with no finishedAt uses ts; today included', () {
      final cols = boardColumns([
        _entry(_makeTask(state: 'done', ts: _kNow - 1000)),
      ], _kNow);
      expect(cols[3].cards, hasLength(1));
    });

    test('done task with no finishedAt uses ts; yesterday excluded', () {
      final cols = boardColumns([
        _entry(_makeTask(state: 'done', ts: _kYesterday)),
      ], _kNow);
      expect(cols[3].cards, isEmpty);
    });

    test('failed task today goes to Done with failed flag', () {
      final cols = boardColumns([
        _entry(_makeTask(state: 'failed', finishedAt: _kNow - 1000)),
      ], _kNow);
      expect(cols[3].cards, hasLength(1));
      expect(cols[3].cards[0].failed, isTrue);
      expect(cols[3].cards[0].cancelled, isFalse);
    });

    test('failed task yesterday is excluded', () {
      final cols = boardColumns([
        _entry(_makeTask(state: 'failed', finishedAt: _kYesterday)),
      ], _kNow);
      expect(cols[3].cards, isEmpty);
    });

    test('cancelled task today goes to Done with cancelled flag', () {
      final cols = boardColumns([
        _entry(_makeTask(state: 'cancelled', finishedAt: _kNow - 500)),
      ], _kNow);
      expect(cols[3].cards, hasLength(1));
      expect(cols[3].cards[0].cancelled, isTrue);
      expect(cols[3].cards[0].failed, isFalse);
    });

    test('cancelled task yesterday is excluded', () {
      final cols = boardColumns([
        _entry(_makeTask(state: 'cancelled', finishedAt: _kYesterday)),
      ], _kNow);
      expect(cols[3].cards, isEmpty);
    });
  });

  // ── sort orders ────────────────────────────────────────────────────────────

  group('boardColumns: sort orders', () {
    test('Queued: oldest first (lowest ts)', () {
      final old = _makeTask(id: 'old', state: 'queued', ts: _kNow - 9000);
      final newer = _makeTask(id: 'new', state: 'queued', ts: _kNow - 1000);
      final cols = boardColumns([_entry(newer), _entry(old)], _kNow);
      expect(cols[0].cards[0].task.id, 'old');
      expect(cols[0].cards[1].task.id, 'new');
    });

    test('Working: longest-running first (lowest startedAt)', () {
      final long = _makeTask(
        id: 'long',
        state: 'running',
        startedAt: _kNow - 60000,
      );
      final short = _makeTask(
        id: 'short',
        state: 'running',
        startedAt: _kNow - 5000,
      );
      final cols = boardColumns([_entry(short), _entry(long)], _kNow);
      expect(cols[1].cards[0].task.id, 'long');
      expect(cols[1].cards[1].task.id, 'short');
    });

    test('Working: task without startedAt falls back to ts', () {
      final a = _makeTask(id: 'a', state: 'running', ts: _kNow - 80000);
      final b = _makeTask(
        id: 'b',
        state: 'running',
        ts: _kNow - 2000,
        startedAt: _kNow - 1000,
      );
      final cols = boardColumns([_entry(b), _entry(a)], _kNow);
      // a has no startedAt → uses ts (_kNow-80000) → ran longer → first
      expect(cols[1].cards[0].task.id, 'a');
    });

    test('Needs you: oldest first (lowest ts)', () {
      final old = _makeTask(
        id: 'old',
        state: 'waiting_action',
        ts: _kNow - 9000,
      );
      final newer = _makeTask(
        id: 'new',
        state: 'waiting_action',
        ts: _kNow - 1000,
      );
      final cols = boardColumns([_entry(newer), _entry(old)], _kNow);
      expect(cols[2].cards[0].task.id, 'old');
    });

    test('Done: newest first (highest finishedAt)', () {
      final earlier = _makeTask(
        id: 'early',
        state: 'done',
        finishedAt: _kNow - 7200000,
      );
      final later = _makeTask(
        id: 'late',
        state: 'done',
        finishedAt: _kNow - 600000,
      );
      final cols = boardColumns([_entry(earlier), _entry(later)], _kNow);
      expect(cols[3].cards[0].task.id, 'late');
      expect(cols[3].cards[1].task.id, 'early');
    });
  });

  // ── column structure ───────────────────────────────────────────────────────

  group('boardColumns: structure', () {
    test('always returns exactly 4 columns in order', () {
      final cols = boardColumns([], _kNow);
      expect(cols, hasLength(4));
      expect(cols.map((c) => c.id).toList(), [
        ColumnId.queued,
        ColumnId.working,
        ColumnId.needsYou,
        ColumnId.done,
      ]);
    });

    test('cards carry hostKey and crewmate reference', () {
      final cols = boardColumns([
        BoardEntry(
          hostKey: 'ssh:atlas-crew@server',
          crewmate: _makeCrewmate(id: 'b1'),
          task: _makeTask(),
        ),
      ], _kNow);
      expect(cols[0].cards[0].hostKey, 'ssh:atlas-crew@server');
      expect(cols[0].cards[0].crewmate.id, 'b1');
    });

    test(
      'midnight boundary: task at exactly todayStart (00:00:00.000) is today',
      () {
        final cols = boardColumns([
          _entry(_makeTask(state: 'done', finishedAt: _kTodayStart)),
        ], _kNow);
        expect(cols[3].cards, hasLength(1));
      },
    );

    test('midnight boundary: task at todayStart - 1ms is yesterday', () {
      final cols = boardColumns([
        _entry(_makeTask(state: 'done', finishedAt: _kTodayStart - 1)),
      ], _kNow);
      expect(cols[3].cards, isEmpty);
    });
  });

  // Incident: the board listed every chat message ("oi", "e agora?", "opaa") as a task.
  // Steps don't tell them apart (a crewmate reads its memory and checks git before any reply),
  // so on a real crew server "oi" had 2 steps. A task is the work the agent names itself
  // ("STORE: Explicar scripts e api.ts da store"); one card per task, however many
  // messages it took.
  group('what is a task', () {
    final stepTs = _kNow ~/ 1000;
    final step = CrewmateStep(ts: stepTs, text: 'read memory.md');

    List<BoardCard> where(List<BoardColumn> cols) =>
        cols.expand((c) => c.cards).toList();
    String label(BoardCard c) => c.title ?? c.task.prompt;

    test('a message the agent never named as work is not a card, even if it ran steps', () {
      final task = CrewmateTask(
        id: 't0',
        ts: _kNow - 60000,
        source: 'desktop',
        prompt: 'oi',
        state: 'done',
        steps: [step],
        finishedAt: _kNow - 1000,
      );
      expect(
        where(boardColumns([_entry(task)], _kNow)).map(label).toList(),
        isEmpty,
      );
    });

    test(
      'a finished task shows the agent\'s name for the work, not the message',
      () {
        final task = CrewmateTask(
          id: 't0',
          ts: _kNow - 60000,
          source: 'desktop',
          prompt: 'lê o package.json',
          state: 'done',
          title: 'STORE: Explicar scripts',
          finishedAt: _kNow - 1000,
        );
        final cards = where(boardColumns([_entry(task)], _kNow));
        expect(cards.map(label).toList(), ['STORE: Explicar scripts']);
        expect(cards[0].task.source, 'desktop');
      },
    );

    test(
      'messages about the same work are one card, in the state of the latest',
      () {
        final t1 = CrewmateTask(
          id: 't1',
          ts: _kNow - 65000,
          source: 'desktop',
          prompt: 'corrige o billing',
          state: 'done',
          title: 'BILL: Wait on billing fix',
          finishedAt: _kNow - 5000,
        );
        final t2 = CrewmateTask(
          id: 't2',
          ts: _kNow - 60000,
          source: 'desktop',
          prompt: 'e agora?',
          state: 'running',
          title: 'BILL: Wait on billing fix',
          steps: [step],
        );
        final cards = where(boardColumns([_entry(t1), _entry(t2)], _kNow));
        expect(cards.map(label).toList(), ['BILL: Wait on billing fix']);
        expect(cards[0].task.state, 'running');
      },
    );

    test('the same title on two crewmates is two tasks', () {
      final a = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(id: 'a', title: 'PR: Review', state: 'running'),
      );
      final b = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(id: 'b', title: 'PR: Review', state: 'running'),
      );
      expect(boardColumns([a, b], _kNow)[1].cards, hasLength(2));
    });

    test('work in flight is shown before the agent names it', () {
      final queued = _entry(
        _makeTask(
          id: 'q',
          source: 'desktop',
          prompt: 'faz o deploy',
          state: 'queued',
        ),
      );
      final running = _entry(
        _makeTask(
          id: 'r',
          source: 'desktop',
          prompt: 'faz o deploy',
          state: 'running',
        ),
      );
      final waiting = _entry(
        _makeTask(
          id: 'w',
          source: 'desktop',
          prompt: 'x',
          state: 'waiting_action',
        ),
      );

      final cq = where(boardColumns([queued], _kNow));
      expect(cq.map((c) => [c.task.state, label(c)]).toList(), [
        ['queued', 'faz o deploy'],
      ]);

      final cr = where(boardColumns([running], _kNow));
      expect(cr.map((c) => [c.task.state, label(c)]).toList(), [
        ['running', 'faz o deploy'],
      ]);

      final cw = where(boardColumns([waiting], _kNow));
      expect(cw.map((c) => [c.task.state, label(c)]).toList(), [
        ['waiting_action', 'x'],
      ]);
    });

    test('an automation run is a task even when the agent gave it no name', () {
      final task = _makeTask(
        id: 't0',
        source: 'trigger',
        prompt: 'Revise os PRs',
        state: 'done',
        finishedAt: _kNow - 1000,
      );
      final cards = where(boardColumns([_entry(task)], _kNow));
      expect(cards.map(label).toList(), ['Revise os PRs']);
    });
  });

  // ── filters ────────────────────────────────────────────────────────────────

  group('boardColumns: filters', () {
    // crewmate filter
    test('crewmate filter: only cards from selected crewmate are shown', () {
      final a = BoardEntry(
        hostKey: 'h1',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(id: 't1', state: 'queued'),
      );
      final b = BoardEntry(
        hostKey: 'h1',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(id: 't2', state: 'queued'),
      );
      final cols = boardColumns(
        [a, b],
        _kNow,
        opts: BoardFilterOptions(crewmates: {'h1\x00b1'}),
      );
      expect(cols.expand((c) => c.cards).toList(), hasLength(1));
      expect(cols.expand((c) => c.cards).first.crewmate.id, 'b1');
    });

    test('crewmate filter: empty Set shows all', () {
      final a = BoardEntry(
        hostKey: 'h1',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(id: 't1', state: 'queued'),
      );
      final b = BoardEntry(
        hostKey: 'h1',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(id: 't2', state: 'queued'),
      );
      final cols = boardColumns(
        [a, b],
        _kNow,
        opts: const BoardFilterOptions(crewmates: {}),
      );
      expect(cols.expand((c) => c.cards).toList(), hasLength(2));
    });

    test('crewmate filter: multiple crewmates selected', () {
      final a = BoardEntry(
        hostKey: 'h1',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(id: 't1', state: 'queued'),
      );
      final b = BoardEntry(
        hostKey: 'h1',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(id: 't2', state: 'queued'),
      );
      final c = BoardEntry(
        hostKey: 'h1',
        crewmate: _makeCrewmate(id: 'b3'),
        task: _makeTask(id: 't3', state: 'queued'),
      );
      final cols = boardColumns(
        [a, b, c],
        _kNow,
        opts: BoardFilterOptions(crewmates: {'h1\x00b1', 'h1\x00b3'}),
      );
      final cards = cols.expand((c) => c.cards).toList();
      expect(cards, hasLength(2));
      expect(cards.map((c) => c.crewmate.id).toList()..sort(), ['b1', 'b3']);
    });

    // source filter
    test("source 'chat': only desktop/mobile tasks", () {
      final chat = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(
          id: 't1',
          source: 'desktop',
          state: 'running',
          title: 'Chat work',
        ),
      );
      final auto = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(id: 't2', source: 'trigger', state: 'running'),
      );
      final cols = boardColumns(
        [chat, auto],
        _kNow,
        opts: const BoardFilterOptions(source: 'chat'),
      );
      final cards = cols.expand((c) => c.cards).toList();
      expect(cards, hasLength(1));
      expect(cards[0].task.source, 'desktop');
    });

    test("source 'auto': only non-chat tasks", () {
      final chat = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(
          id: 't1',
          source: 'desktop',
          state: 'running',
          title: 'Chat work',
        ),
      );
      final auto1 = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(id: 't2', source: 'trigger', state: 'running'),
      );
      final auto2 = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b3'),
        task: _makeTask(id: 't3', source: 'routine', state: 'running'),
      );
      final cols = boardColumns(
        [chat, auto1, auto2],
        _kNow,
        opts: const BoardFilterOptions(source: 'auto'),
      );
      final cards = cols.expand((c) => c.cards).toList();
      expect(cards, hasLength(2));
      expect(
        cards.every(
          (c) => c.task.source != 'desktop' && c.task.source != 'mobile',
        ),
        isTrue,
      );
    });

    test("source 'all': no filtering applied", () {
      final a = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(
          id: 't1',
          source: 'desktop',
          state: 'running',
          title: 'A',
        ),
      );
      final b = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(id: 't2', source: 'trigger', state: 'running'),
      );
      final cols = boardColumns(
        [a, b],
        _kNow,
        opts: const BoardFilterOptions(source: 'all'),
      );
      expect(cols.expand((c) => c.cards).toList(), hasLength(2));
    });

    // query filter
    test('query: matches title case-insensitively', () {
      final a = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(
          id: 't1',
          state: 'queued',
          title: 'STORE: Deploy scripts',
          prompt: 'deploy',
        ),
      );
      final b = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(
          id: 't2',
          state: 'queued',
          title: 'BILL: Fix billing',
          prompt: 'billing',
        ),
      );
      final cols = boardColumns(
        [a, b],
        _kNow,
        opts: const BoardFilterOptions(query: 'store'),
      );
      final cards = cols.expand((c) => c.cards).toList();
      expect(cards, hasLength(1));
      expect(cards[0].title, 'STORE: Deploy scripts');
    });

    test('query: matches prompt when no title', () {
      final a = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(
          id: 't1',
          source: 'trigger',
          state: 'queued',
          prompt: 'Fix the database',
        ),
      );
      final b = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(
          id: 't2',
          source: 'trigger',
          state: 'queued',
          prompt: 'Review the PR',
        ),
      );
      final cols = boardColumns(
        [a, b],
        _kNow,
        opts: const BoardFilterOptions(query: 'database'),
      );
      final cards = cols.expand((c) => c.cards).toList();
      expect(cards, hasLength(1));
      expect(cards[0].task.prompt, 'Fix the database');
    });

    test('query: no match returns empty columns', () {
      final a = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(id: 't1', state: 'queued', title: 'STORE: Deploy'),
      );
      final cols = boardColumns(
        [a],
        _kNow,
        opts: const BoardFilterOptions(query: 'xxxxxxnotfound'),
      );
      expect(cols.expand((c) => c.cards).toList(), isEmpty);
    });

    // doneDays
    test('doneDays 7: includes a task finished 6 days ago', () {
      final sixDaysAgo = _kNow - 6 * 24 * 60 * 60 * 1000;
      final cols = boardColumns(
        [_entry(_makeTask(state: 'done', finishedAt: sixDaysAgo))],
        _kNow,
        opts: const BoardFilterOptions(doneDays: 7),
      );
      expect(cols[3].cards, hasLength(1));
    });

    test('doneDays 7: excludes a task finished 8 days ago', () {
      final eightDaysAgo = _kNow - 8 * 24 * 60 * 60 * 1000;
      final cols = boardColumns(
        [_entry(_makeTask(state: 'done', finishedAt: eightDaysAgo))],
        _kNow,
        opts: const BoardFilterOptions(doneDays: 7),
      );
      expect(cols[3].cards, isEmpty);
    });

    test('doneDays 7: still includes a task from today', () {
      final cols = boardColumns(
        [_entry(_makeTask(state: 'done', finishedAt: _kNow - 3600000))],
        _kNow,
        opts: const BoardFilterOptions(doneDays: 7),
      );
      expect(cols[3].cards, hasLength(1));
    });

    test('doneDays 1 (default): task at yesterday is excluded even if within rolling 24h', () {
      final cols = boardColumns(
        [_entry(_makeTask(state: 'done', finishedAt: _kYesterday))],
        _kNow,
        opts: const BoardFilterOptions(doneDays: 1),
      );
      expect(cols[3].cards, isEmpty);
    });

    // combined
    test('combined crewmate + source: intersection', () {
      final b1chat = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(
          id: 't1',
          source: 'desktop',
          state: 'running',
          title: 'Chat b1',
        ),
      );
      final b1auto = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b1'),
        task: _makeTask(id: 't2', source: 'trigger', state: 'running'),
      );
      final b2chat = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(
          id: 't3',
          source: 'desktop',
          state: 'running',
          title: 'Chat b2',
        ),
      );
      final cols = boardColumns(
        [b1chat, b1auto, b2chat],
        _kNow,
        opts: BoardFilterOptions(crewmates: {'\x00b1'}, source: 'chat'),
      );
      final cards = cols.expand((c) => c.cards).toList();
      expect(cards, hasLength(1));
      expect(cards[0].crewmate.id, 'b1');
      expect(cards[0].task.source, 'desktop');
    });

    test('combined query + doneDays 7: both filters applied', () {
      final sixDaysAgo = _kNow - 6 * 24 * 60 * 60 * 1000;
      final old = _entry(
        _makeTask(
          state: 'done',
          finishedAt: sixDaysAgo,
          title: 'STORE: Old task',
        ),
      );
      final recent = BoardEntry(
        hostKey: '',
        crewmate: _makeCrewmate(id: 'b2'),
        task: _makeTask(
          id: 't2',
          state: 'done',
          finishedAt: _kNow - 3600000,
          title: 'BILL: Recent',
        ),
      );
      final cols = boardColumns(
        [old, recent],
        _kNow,
        opts: const BoardFilterOptions(doneDays: 7, query: 'store'),
      );
      expect(cols[3].cards, hasLength(1));
      expect(cols[3].cards[0].title, 'STORE: Old task');
    });
  });
}
