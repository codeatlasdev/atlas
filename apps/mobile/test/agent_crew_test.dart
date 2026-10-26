import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/crew_widgets.dart';

import 'fixtures/demo_computer.dart';

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// Wraps the demo computer and records every call, so a test asserts what the phone sent.
class _Spy implements Transport {
  _Spy(this.inner, {this.replies = const {}}) {
    inner.incomingLines.listen(_lines.add);
  }
  final Transport inner;

  /// cmd → data, answered instead of the demo (to script a tree or a status).
  final Map<String, Object? Function(Map<String, dynamic>)> replies;
  final calls = <(String, Map<String, dynamic>)>[];
  final _lines = StreamController<String>.broadcast();

  @override
  Stream<String> get incomingLines => _lines.stream;

  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    final cmd = req['cmd'] as String;
    final args = (req['args'] as Map?)?.cast<String, dynamic>() ?? const {};
    calls.add((cmd, args));
    final o = replies[cmd];
    if (o == null) return inner.sendLine(line);
    final r = o(args);
    scheduleMicrotask(
      () => _lines.add(
        jsonEncode(
          r is String
              ? {'id': req['id'], 'ok': false, 'error': r}
              : {'id': req['id'], 'ok': true, 'data': r},
        ),
      ),
    );
  }

  @override
  Future<void> close() async {
    await inner.close();
    await _lines.close();
  }

  Iterable<Map<String, dynamic>> sent(String cmd) =>
      calls.where((c) => c.$1 == cmd).map((c) => c.$2);
}

Future<void> _settle(WidgetTester t, [int ms = 600]) async {
  for (var i = 0; i < ms ~/ 50; i++) {
    await t.pump(const Duration(milliseconds: 50));
  }
}

Future<void> _pump(WidgetTester t, Transport pc, Widget home) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  addTearDown(() => t.binding.setSurfaceSize(null));
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage()),
        transportFactoryProvider.overrideWithValue((_, _) async => pc),
      ],
      child: MaterialApp(theme: atlasTheme(), home: home),
    ),
  );
  await _settle(t, 800);
}

Future<void> _unmount(WidgetTester t) async {
  await t.pumpWidget(const SizedBox());
  await _settle(t, 200);
}

const _mac = MacDevice(nodeId: 'n1', name: 'My Mac');

AgentStatus _st(String agent, String state) => AgentStatus.fromJson({
  'state': state,
  'agent': agent,
  'model': null,
  'updated_at': 1000,
  'state_started_at': DateTime.now().millisecondsSinceEpoch,
  'title': 'T',
});

Widget _chat(String pane, AgentStatus s, {String? sub}) => AgentChatScreen(
  device: _mac,
  paneKey: pane,
  initial: s,
  title: 'agent',
  sub: sub,
);

Map<String, dynamic> _treeFixture() =>
    jsonDecode(File('test/fixtures/agent_tree.json').readAsStringSync())
        as Map<String, dynamic>;

void main() {
  // Real hostd output: a mock could hide a renamed key.
  group('agent_tree contract', () {
    test('the producer output parses into nodes with their real fields', () {
      final t = AgentTree.fromJson(_treeFixture());
      expect(t.nodes.map((n) => n.id), ['', 'Fixer', 'Scout', 'Scout.Deep']);
      final root = t.node('')!;
      expect(
        (root.isRoot, root.running, root.label),
        (true, true, 'Map the host'),
      );
      expect(root.durationMs, 0, reason: 'a null duration reads as zero');
      expect(root.tokens, 1200);
      final scout = t.node('Scout')!;
      expect(
        (scout.now, scout.running, scout.parent),
        ('Reading server.rs', true, ''),
      );
      final deep = t.node('Scout.Deep')!;
      expect(
        (deep.label, deep.done, deep.parent, deep.report),
        ('Deep', true, 'Scout', 'server.rs routes every command'),
      );
      final fixer = t.node('Fixer')!;
      expect((fixer.failed, fixer.durationMs), (true, 65000));
    });

    test('the hub fields of the producer output parse', () {
      final t = AgentTree.fromJson(_treeFixture());
      final root = t.node('')!;
      expect((root.thinking, root.agentType, root.requests), ('high', null, 1));
      final todos = root.todos!;
      expect((todos.done, todos.total), (1, 3));
      expect(todos.current!.content, 'Write the hub');
      expect(todos.phases.map((p) => p.name), ['Map', 'Ship']);
      final scout = t.node('Scout')!;
      expect(
        (scout.agentType, scout.thinking, scout.requests, scout.activeMs),
        ('scout', 'low', 2, 9000),
      );
      expect((scout.contextTokens, scout.contextWindow), (292000, 1000000));
      expect(scout.contextShare, closeTo(0.292, 1e-9));
      expect(scout.recent.map((r) => (r.tool, r.status)), [
        ('task', 'ok'),
        ('read', 'running'),
      ]);
      expect(scout.recent.last.intent, 'Reading server.rs');
      expect(scout.recent.last.at, greaterThan(0));
      // A node without figures has no context bar.
      expect(t.node('Fixer')!.contextShare, isNull);
      // An older hostd sends none of it: the defaults hold.
      final old = AgentNode.fromJson({
        'id': 'A',
        'parent': '',
        'status': 'done',
      });
      expect(old.recent, isEmpty);
      expect((old.todos, old.contextShare, old.requests), (null, null, 0));
    });

    test('flat is depth-first with depths; path walks to the root', () {
      final t = AgentTree.fromJson(_treeFixture());
      expect(
        [for (final (n, d) in t.flat) (n.id, d)],
        [('', 0), ('Fixer', 1), ('Scout', 1), ('Scout.Deep', 2)],
      );
      expect(t.path('Scout.Deep').map((n) => n.id), [
        '',
        'Scout',
        'Scout.Deep',
      ]);
      expect(t.siblings('Scout').map((n) => n.id), ['Fixer', 'Scout']);
      expect(t.siblings('').map((n) => n.id), ['']);
    });

    test('summary counts the subagents, not the root', () {
      final t = AgentTree.fromJson(_treeFixture());
      expect((t.done, t.running, t.failed), (1, 1, 1));
      expect(t.summary, '1/3 done · 1 running · 1 failed');
      expect(AgentTree.fromJson({'nodes': []}).summary, '');
    });

    test('a node whose parent is missing hangs off the root', () {
      final t = AgentTree.fromJson({
        'nodes': [
          {'id': '', 'parent': null, 'status': 'running'},
          {'id': 'A.B', 'parent': 'A', 'status': 'running'},
        ],
      });
      expect([for (final (n, d) in t.flat) (n.id, d)], [('', 0), ('A.B', 1)]);
    });
  });

  group('timeline items', () {
    test('result and compaction parse; unknown kinds stay skipped', () {
      final h = AgentHistory.fromJson({
        'items': [
          {
            'kind': 'result',
            'id': 'Scout.Deep',
            'status': 'failed',
            'duration': '3s',
          },
          {'kind': 'compaction', 'text': 'folded 40 turns'},
        ],
      });
      final r = h.items[0] as ResultItem;
      expect((r.id, r.status, r.duration), ('Scout.Deep', 'failed', '3s'));
      expect((h.items[1] as CompactionItem).text, 'folded 40 turns');
    });
  });

  group('recap', () {
    const edit = ToolItem(
      id: '1',
      name: 'edit',
      detail: 'a.dart',
      status: 'ok',
    );
    test('counts files, commands and subagent results of a turn', () {
      final r = Recap.of(const [
        UserItem('go'),
        edit,
        ToolItem(id: '2', name: 'write', detail: 'b.dart', status: 'ok'),
        ToolItem(id: '3', name: 'edit', detail: 'a.dart', status: 'ok'),
        ToolItem(id: '4', name: 'edit', detail: 'c.dart', status: 'error'),
        ToolItem(id: '5', name: 'bash', detail: 'ls', status: 'ok'),
        ToolItem(id: '6', name: 'bash', detail: 'pwd', status: 'error'),
        ResultItem(id: 'S', status: 'completed'),
        ResultItem(id: 'T', status: 'failed'),
        ResultItem(id: 'U', status: 'aborted'),
      ]);
      expect(r.files, [
        'a.dart',
        'b.dart',
      ], reason: 'distinct, failed excluded');
      expect(r.commands, 2);
      expect((r.finished, r.failed), (1, 2));
      expect(r.isEmpty, isFalse);
      expect(
        Recap.of(const [UserItem('hi'), AssistantItem('yo')]).isEmpty,
        isTrue,
      );
    });

    test(
      'each finished turn gets its recap under its prompt; the live one waits',
      () {
        const items = <TimelineItem>[
          UserItem('one'),
          edit,
          AssistantItem('done one'),
          UserItem('two'),
          ToolItem(id: '5', name: 'bash', detail: 'ls', status: 'ok'),
        ];
        List<String> kinds(List<TimelineBlock> b) => [
          for (final x in b) x.runtimeType.toString(),
        ];
        expect(kinds(groupTimeline(items)), [
          'ItemBlock',
          'RecapBlock',
          'ToolGroup',
          'ItemBlock',
          'ItemBlock',
          'ToolGroup',
        ]);
        expect(kinds(groupTimeline(items, recapLast: true)), [
          'ItemBlock',
          'RecapBlock',
          'ToolGroup',
          'ItemBlock',
          'ItemBlock',
          'RecapBlock',
          'ToolGroup',
        ]);
      },
    );
  });

  group('crew in the chat', () {
    testWidgets('the rail lists the nested crew with each status', (t) async {
      final pc = _Spy(demoTransport());
      await _pump(t, pc, _chat('demo-omp', _st('omp', 'done')));

      expect(pc.sent('agent_tree').first, {'pane': 'demo-omp'});
      expect(find.byType(CrewRail), findsOneWidget);
      expect(find.text('LedgerScout'), findsWidgets);
      expect(
        find.descendant(of: find.byType(CrewRail), matching: find.text('Deep')),
        findsOneWidget,
        reason:
            'the chip shows the last dotted segment, the id rides on semantics',
      );
      // The main agent and both subagents ended well.
      expect(find.text('done'), findsWidgets);
      expect(find.byKey(const ValueKey('crew-LedgerScout')), findsOneWidget);
      expect(
        find.byKey(const ValueKey('crew-LedgerScout.Deep')),
        findsOneWidget,
      );
      await _unmount(t);
    });

    testWidgets(
      'tapping a nested node reads its transcript and shows the breadcrumb',
      (t) async {
        final pc = _Spy(demoTransport());
        await _pump(t, pc, _chat('demo-omp', _st('omp', 'done')));

        await t.ensureVisible(
          find.byKey(const ValueKey('crew-LedgerScout.Deep')),
        );
        await t.tap(find.byKey(const ValueKey('crew-LedgerScout.Deep')));
        await _settle(t, 800);

        expect(
          pc.sent('agent_history').any((a) => a['sub'] == 'LedgerScout.Deep'),
          isTrue,
          reason: 'the nested id goes to hostd as one dotted name',
        );
        expect(find.text('ledger url: localhost:4300'), findsOneWidget);
        // Main › LedgerScout › Deep, always on screen.
        final crumbs = find.descendant(
          of: find.byType(Breadcrumb),
          matching: find.byType(Text),
        );
        expect(
          [for (final w in crumbs.evaluate()) (w.widget as Text).data],
          ['Main', 'LedgerScout', 'Deep'],
        );
        expect(
          find.text('Message Main about Deep'),
          findsOneWidget,
          reason: 'the composer says who it reaches',
        );

        // A crumb goes back up.
        await t.tap(find.byKey(const ValueKey('crumb-LedgerScout')));
        await _settle(t, 800);
        expect(pc.sent('agent_history').last['sub'], 'LedgerScout');
        // One tap on Main returns to the main agent, whose composer is plain again.
        await t.tap(find.byKey(const ValueKey('crumb-')));
        await _settle(t, 800);
        expect(find.byType(Breadcrumb), findsNothing);
        expect(pc.sent('agent_history').last.containsKey('sub'), isFalse);
        expect(find.text('Message'), findsOneWidget);
        await _unmount(t);
      },
    );

    testWidgets('a finished-subagent card focuses that subagent', (t) async {
      final pc = _Spy(demoTransport());
      await _pump(t, pc, _chat('demo-omp', _st('omp', 'done')));

      final card = find.textContaining('finished');
      await t.ensureVisible(card.first);
      await t.tap(card.first);
      await _settle(t, 800);

      expect(
        pc.sent('agent_history').any((a) => a['sub'] == 'LedgerScout'),
        isTrue,
      );
      await _unmount(t);
    });

    testWidgets('long-press opens the agent detail', (t) async {
      final pc = _Spy(demoTransport());
      await _pump(t, pc, _chat('demo-omp', _st('omp', 'done')));

      await t.ensureVisible(
        find.byKey(const ValueKey('crew-LedgerScout.Deep')),
      );
      await t.longPress(find.byKey(const ValueKey('crew-LedgerScout.Deep')));
      await _settle(t, 500);
      expect(find.text('Task'), findsOneWidget);
      expect(
        find.text('Read payments/atlas.yaml and report the ledger URL'),
        findsOneWidget,
      );
      expect(find.text('Usage'), findsOneWidget);
      expect(find.text('Lineage'), findsOneWidget);
      expect(find.text('Report'), findsOneWidget);
      expect(find.text('Open conversation'), findsOneWidget);
      await _unmount(t);
    });

    testWidgets('the hub opens from the app bar and lists the crew', (t) async {
      final pc = _Spy(
        demoTransport(),
        replies: {'agent_tree': (_) => _treeFixture()},
      );
      await _pump(t, pc, _chat('demo-omp', _st('omp', 'working')));

      await t.tap(find.byTooltip('Agent hub').first);
      await _settle(t, 600);
      expect(find.text('Agent hub'), findsOneWidget);
      // Root and Scout run; Fixer failed and Deep is done: 2 running, 2 parked.
      expect(find.text('2 running · 2 parked'), findsOneWidget);
      expect(find.byKey(const ValueKey('hub-Scout')), findsOneWidget);
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('hub-Scout')),
          matching: find.textContaining('Reading server.rs'),
        ),
        findsOneWidget,
        reason: 'the live line of what runs',
      );

      await t.tap(find.byKey(const ValueKey('hub-Scout.Deep')));
      await _settle(t, 500);
      await t.tap(find.byKey(const ValueKey('detail-open')));
      await _settle(t, 800);
      expect(find.text('Agent hub'), findsNothing, reason: 'back to the chat');
      expect(pc.sent('agent_history').last['sub'], 'Scout.Deep');
      await _unmount(t);
    });

    testWidgets('the now strip shows what the focused agent is doing', (
      t,
    ) async {
      final pc = _Spy(
        demoTransport(),
        replies: {'agent_tree': (_) => _treeFixture()},
      );
      await _pump(t, pc, _chat('demo-omp', _st('omp', 'working')));
      expect(
        find.byKey(const ValueKey('jobs-title')),
        findsOneWidget,
        reason: 'the main agent waits on Scout',
      );
      await t.tap(find.byKey(const ValueKey('crew-Scout')));
      await _settle(t, 800);
      expect(find.text('Reading server.rs'), findsWidgets);
      await _unmount(t);
    });

    testWidgets('swiping the timeline moves to the next sibling', (t) async {
      final pc = _Spy(
        demoTransport(),
        replies: {'agent_tree': (_) => _treeFixture()},
      );
      await _pump(
        t,
        pc,
        _chat('demo-omp', _st('omp', 'working'), sub: 'Fixer'),
      );
      await t.fling(find.byType(ListView).last, const Offset(-300, 0), 1200);
      await _settle(t, 800);
      expect(pc.sent('agent_history').last['sub'], 'Scout');
      await _unmount(t);
    });

    testWidgets('the main agent shows what it waits on and its todos', (
      t,
    ) async {
      final pc = _Spy(
        demoTransport(),
        replies: {'agent_tree': (_) => _treeFixture()},
      );
      await _pump(t, pc, _chat('demo-omp', _st('omp', 'working')));
      expect(find.text('Waiting on 1 job'), findsOneWidget);
      expect(find.byKey(const ValueKey('job-Scout')), findsOneWidget);
      // Collapsed: the task in progress and the count.
      expect(find.text('1/3'), findsOneWidget);
      expect(find.text('Write the hub'), findsOneWidget);
      await t.tap(find.byKey(const ValueKey('todo-toggle')));
      await _settle(t, 300);
      expect(find.text('Read hostd'), findsOneWidget);

      // A job is a link into that agent; on a subagent the root strips are gone.
      await t.tap(find.byKey(const ValueKey('job-Scout')));
      await _settle(t, 800);
      expect(pc.sent('agent_history').last['sub'], 'Scout');
      expect(find.textContaining('Waiting on'), findsNothing);
      expect(find.byKey(const ValueKey('todo-toggle')), findsNothing);
      await _unmount(t);
    });

    testWidgets('an omp subagent message goes to the main agent, labelled', (
      t,
    ) async {
      final pc = _Spy(
        demoTransport(),
        replies: {
          'agent_tree': (_) => _treeFixture(),
          'agent_send': (_) => <String, Object?>{},
        },
      );
      await _pump(
        t,
        pc,
        _chat('demo-omp', _st('omp', 'done'), sub: 'Scout.Deep'),
      );
      expect(find.text('Message Main about Deep'), findsOneWidget);
      await t.enterText(find.byType(TextField), 'also check the lock');
      await t.tap(find.byTooltip('Send message'));
      await _settle(t, 400);
      expect(pc.sent('agent_send').single, {
        'pane': 'demo-omp',
        'text': 'Tell Scout.Deep: also check the lock',
      });
      await _unmount(t);
    });

    testWidgets('Claude has no composer on a subagent', (t) async {
      final pc = _Spy(
        demoTransport(),
        replies: {
          'agent_tree': (_) => {
            'nodes': [
              {'id': '', 'parent': null, 'status': 'done', 'agent': 'claude'},
              {
                'id': 'agent-1',
                'parent': '',
                'status': 'done',
                'name': 'Explore',
              },
            ],
          },
        },
      );
      await _pump(
        t,
        pc,
        _chat('demo-claude', _st('claude', 'done'), sub: 'agent-1'),
      );
      expect(find.byType(TextField), findsNothing, reason: 'nothing relays it');
      expect(find.byKey(const ValueKey('crumb-')), findsOneWidget);
      await _unmount(t);
    });

    testWidgets(
      'the hub, opened from the chat, can start a message about a node',
      (t) async {
        final pc = _Spy(
          demoTransport(),
          replies: {'agent_tree': (_) => _treeFixture()},
        );
        await _pump(t, pc, _chat('demo-omp', _st('omp', 'done')));
        await t.tap(find.byTooltip('Agent hub').first);
        await _settle(t, 600);
        await t.tap(find.byKey(const ValueKey('hub-Scout')));
        await _settle(t, 600);
        await t.tap(find.byKey(const ValueKey('detail-message')));
        await _settle(t, 1500);
        expect(find.text('Agent hub'), findsNothing);
        expect(pc.sent('agent_history').last['sub'], 'Scout');
        expect(find.text('Message Main about Scout'), findsOneWidget);
        final field = t.widget<TextField>(find.byType(TextField));
        expect(field.focusNode!.hasFocus, isTrue, reason: 'ready to type');
        await _unmount(t);
      },
    );

    testWidgets('a Claude agent without subagents has no crew chrome', (
      t,
    ) async {
      final pc = _Spy(demoTransport());
      await _pump(t, pc, _chat('demo-claude', _st('claude', 'blocked')));
      expect(find.byType(CrewRail), findsNothing);
      expect(find.byTooltip('Agent hub'), findsOneWidget);
      await _unmount(t);
    });

    testWidgets('a hostd without agent_tree still chats', (t) async {
      final pc = _Spy(
        demoTransport(),
        replies: {'agent_tree': (_) => 'unknown command'},
      );
      await _pump(t, pc, _chat('demo-omp', _st('omp', 'done')));
      expect(find.byType(CrewRail), findsNothing);
      expect(find.text('Why is payments failing?'), findsWidgets);
      await _unmount(t);
    });
  });

  group('steer', () {
    Map<String, Object? Function(Map<String, dynamic>)> working(String agent) =>
        {
          'agent_status_all': (_) => {
            'panes': {
              'p': {
                'state': 'working',
                'agent': agent,
                'updated_at': 1000,
                'state_started_at': DateTime.now().millisecondsSinceEpoch,
                'title': 'T',
              },
            },
            'hook_port': 0,
            'pending_approvals': <Object>[],
          },
          'agent_tree': (_) => {'nodes': <Object>[]},
          'agent_history': (_) => {'items': <Object>[], 'messages': <Object>[]},
          'agent_send': (_) => <String, Object?>{},
        };

    testWidgets('omp working: the composer stays on and sends steer:true', (
      t,
    ) async {
      final pc = _Spy(demoTransport(), replies: working('omp'));
      await _pump(t, pc, _chat('p', _st('omp', 'working')));

      expect(find.text('Steer the agent…'), findsOneWidget);
      expect(
        find.byTooltip('Stop'),
        findsOneWidget,
        reason: 'Stop stays apart',
      );
      await t.enterText(find.byType(TextField), 'use the other API');
      await t.tap(find.byTooltip('Steer'));
      await _settle(t, 400);
      expect(pc.sent('agent_send').single, {
        'pane': 'p',
        'text': 'use the other API',
        'steer': true,
      });
      await _unmount(t);
    });

    testWidgets('claude working: no composer, no steer', (t) async {
      final pc = _Spy(demoTransport(), replies: working('claude'));
      await _pump(t, pc, _chat('p', _st('claude', 'working')));

      expect(find.text('Steer the agent…'), findsNothing);
      expect(find.byTooltip('Steer'), findsNothing);
      expect(
        t.widget<TextField>(find.byType(TextField)).enabled,
        isFalse,
        reason: 'only omp takes a message while it works',
      );
      await _unmount(t);
    });

    testWidgets('omp idle sends a plain message (no steer flag)', (t) async {
      final pc = _Spy(
        demoTransport(),
        replies: {
          ...working('omp'),
          'agent_status_all': (_) => {
            'panes': {
              'p': {
                'state': 'done',
                'agent': 'omp',
                'updated_at': 1000,
                'state_started_at': 0,
                'title': 'T',
              },
            },
            'hook_port': 0,
            'pending_approvals': <Object>[],
          },
        },
      );
      await _pump(t, pc, _chat('p', _st('omp', 'done')));
      await t.enterText(find.byType(TextField), 'next');
      await t.tap(find.byTooltip('Send message'));
      await _settle(t, 400);
      expect(pc.sent('agent_send').single, {'pane': 'p', 'text': 'next'});
      await _unmount(t);
    });
  });

  group('recap card', () {
    testWidgets('a finished turn shows files, commands and subagents', (
      t,
    ) async {
      final pc = _Spy(demoTransport());
      await _pump(t, pc, _chat('demo-omp', _st('omp', 'done')));
      await t.fling(find.byType(ListView).last, const Offset(0, 2000), 3000);
      await _settle(t, 600);
      expect(find.text('Recap'), findsOneWidget);
      expect(find.text('1 file changed'), findsOneWidget);
      expect(find.text('2 commands'), findsOneWidget);
      expect(find.text('1 subagent done'), findsOneWidget);
      expect(find.text('payments/atlas.yaml'), findsOneWidget);
      await _unmount(t);
    });
  });

  group('formatters', () {
    test('durations, tokens and cost read like a dashboard', () {
      expect(durationText(0), '');
      expect(durationText(41000), '41s');
      expect(durationText(184000), '3m 4s');
      expect(durationText(3720000), '1h 2m');
      expect(tokensText(812), '812');
      expect(tokensText(48200), '48.2k');
      expect(costText(0), '');
      expect(costText(0.004), r'$0.004');
      expect(costText(0.31), r'$0.31');
    });
  });
}
