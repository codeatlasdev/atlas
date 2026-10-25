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
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/agent_launcher.dart';
import 'package:atlas_mobile/features/projects/project_detail_screen.dart';

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// A computer that answers by command and records every call it gets, so a test can
/// assert what the phone sent and script what hostd replies.
class _Computer implements Transport {
  _Computer(this.replies);

  /// cmd → reply data (or a function of args); a [String] is an error.
  final Map<String, Object? Function(Map<String, dynamic> args)> replies;
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
    final r = replies[cmd]?.call(args) ?? <String, Object?>{};
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
  Future<void> close() async => _lines.close();

  Iterable<Map<String, dynamic>> sent(String cmd) =>
      calls.where((c) => c.$1 == cmd).map((c) => c.$2);
}

const _mac = MacDevice(nodeId: 'n1', name: 'My Mac');

Map<String, dynamic> _fixture() =>
    jsonDecode(File('test/fixtures/agent_history.json').readAsStringSync())
        as Map<String, dynamic>;

Map<String, Object?> _status(
  String agent,
  String state, {
  String? title,
  String? prompt,
}) => {
  'state': state,
  'agent': agent,
  'model': agent == 'omp' ? 'gpt-5.5' : null,
  'prompt': prompt,
  'updated_at': 1000,
  'state_started_at': DateTime.now().millisecondsSinceEpoch,
  'title': title,
};

AgentStatus _initial(String agent, String state, {String? title}) =>
    AgentStatus.fromJson(_status(agent, state, title: title));

Future<void> _pump(WidgetTester t, _Computer pc, Widget home) async {
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
  await _settle(t);
}

Future<void> _settle(WidgetTester t, [int ms = 600]) async {
  for (var i = 0; i < ms ~/ 50; i++) {
    await t.pump(const Duration(milliseconds: 50));
  }
}

Future<void> _unmount(WidgetTester t) async {
  await t.pumpWidget(const SizedBox());
  await _settle(t, 200);
}

Map<String, Object? Function(Map<String, dynamic>)> _base({
  String state = 'done',
  Map<String, dynamic>? history,
  Map<String, Map<String, Object?>>? panes,
}) => {
  'agent_status_all': (_) => {
    'panes':
        panes ?? {'p1': _status('omp', state, title: 'Fix the sheet z-index')},
    'hook_port': 0,
    'pending_approvals': <Object>[],
  },
  'agent_history': (_) => history ?? _fixture(),
};

Widget _chat({
  String state = 'done',
  String pane = 'p1',
  String agent = 'omp',
  String? sub,
}) => AgentChatScreen(
  device: _mac,
  paneKey: pane,
  initial: _initial(agent, state),
  title: sub ?? 'agent',
  sub: sub,
);

void main() {
  // Real hostd output (atlas-host's mobile tests write it): a mock could hide a renamed
  // key, as a camelCase mock once hid a snake_case field in the desktop.
  group('agent_history contract', () {
    test('the producer output parses into what the timeline reads', () {
      final h = AgentHistory.fromJson(_fixture());
      expect(h.title, 'Fix the sheet z-index');
      expect(h.truncated, isFalse);
      expect(h.items.map((i) => i.runtimeType.toString()), [
        'UserItem',
        'ThinkingItem',
        'AssistantItem',
        'ToolItem',
        'ToolItem',
        'AssistantItem',
      ]);
      final bash = h.items[3] as ToolItem;
      expect(
        (bash.id, bash.name, bash.status, bash.output),
        ('a', 'bash', 'error', '1 error'),
      );
      expect(bash.summary, 'Running biome', reason: 'the intent wins');
      expect(bash.detail, 'bunx biome check --write');
      final task = h.items[4] as ToolItem;
      expect(task.isTask, isTrue);
      expect(
        task.summary,
        'task',
        reason: 'no intent, no detail: the tool name',
      );
      final spawned = task.agents.single;
      expect(
        (spawned.name, spawned.agent, spawned.task),
        ('CssScout', 'scout', 'Find z-index rules'),
      );
      final sub = h.subagent('CssScout')!;
      expect(
        (sub.state, sub.running, sub.report, sub.updatedAt),
        ('done', false, 'nav z-40, sheet z-50', 1700000000000),
      );
      expect(h.subagent('nobody'), isNull);
    });

    test(
      'consecutive plain tool calls fold into one group; task calls stay apart',
      () {
        final blocks = groupTimeline(AgentHistory.fromJson(_fixture()).items);
        expect(blocks.map((b) => b.runtimeType.toString()), [
          'ItemBlock',
          'ItemBlock',
          'ItemBlock',
          'ToolGroup',
          'SubagentsBlock',
          'ItemBlock',
        ]);
        final group = blocks[3] as ToolGroup;
        expect(group.calls.single.name, 'bash');
        expect((group.errors, group.running), (1, false));

        const t = ToolItem(id: '1', name: 'read', status: 'ok');
        const u = ToolItem(id: '2', name: 'bash', status: 'running');
        final run = groupTimeline(const [t, u, t, UserItem('x'), t]);
        expect(run.length, 3);
        expect((run[0] as ToolGroup).names, ['read', 'bash']);
        expect((run[0] as ToolGroup).running, isTrue);
      },
    );

    test('an older hostd without items falls back to its messages', () {
      final h = AgentHistory.fromJson({
        'messages': [
          {'role': 'user', 'text': 'hi'},
          {'role': 'assistant', 'text': 'hello'},
        ],
      });
      expect(h.items.map((i) => (i as dynamic).text), ['hi', 'hello']);
      expect(h.items.first, isA<UserItem>());
      expect(h.items.last, isA<AssistantItem>());
    });

    test('an unknown item kind is skipped, not a crash', () {
      final h = AgentHistory.fromJson({
        'items': [
          {'kind': 'hologram', 'text': '?'},
          {'kind': 'user', 'text': 'ok'},
        ],
      });
      expect(h.items.single, isA<UserItem>());
    });

    test('AgentStatus carries the session title and compares by it', () {
      final a = AgentStatus.fromJson(_status('omp', 'idle', title: 'T'));
      final b = AgentStatus.fromJson(_status('omp', 'idle', title: 'U'));
      expect(a.title, 'T');
      expect(a == b, isFalse, reason: 'same state and time, new title');
      expect(AgentStatus.fromJson(_status('omp', 'idle')).title, isNull);
    });
  });

  testWidgets('timeline: bubble, collapsed thought, grouped steps, subagents', (
    t,
  ) async {
    final pc = _Computer(_base());
    await _pump(t, pc, _chat());

    // Session title in the bar, the agent's logo beside it.
    expect(find.text('Fix the sheet z-index'), findsOneWidget);
    expect(find.byType(AgentLogo), findsWidgets);

    // The user's prompt is a bubble; the thought is collapsed to one line.
    expect(find.text('O sheet fica sob a nav. Corrige?'), findsOneWidget);
    expect(find.text('Thought'), findsOneWidget);
    expect(find.text('Compare z-index of nav and sheet.'), findsNothing);
    await t.tap(find.text('Thought'));
    await t.pump();
    expect(find.text('Compare z-index of nav and sheet.'), findsOneWidget);

    // One bash call that failed: a card that says so, collapsed until tapped.
    expect(find.text('1 step · bash'), findsOneWidget);
    expect(find.text('1 failed'), findsOneWidget);
    expect(find.text('Running biome'), findsNothing);
    await t.tap(find.text('1 step · bash'));
    await t.pump();
    expect(find.text('Running biome'), findsOneWidget);
    expect(find.byIcon(Icons.close_rounded), findsOneWidget);
    await t.tap(find.text('Running biome'));
    await t.pump();
    expect(find.textContaining('1 error'), findsOneWidget);
    expect(find.textContaining(r'$ bunx biome check --write'), findsOneWidget);

    // The task call is a Subagents card with the live state and the report preview.
    expect(find.text('1 subagent'), findsOneWidget);
    expect(find.text('CssScout'), findsOneWidget);
    expect(find.text('done'), findsWidgets);
    expect(find.text('nav z-40, sheet z-50'), findsOneWidget);

    // Both assistant texts rendered, not squeezed into bubbles.
    expect(find.textContaining('Pronto'), findsOneWidget);
    await _unmount(t);
  });

  testWidgets('tapping a subagent opens its transcript read-only', (t) async {
    final pc = _Computer({
      ..._base(),
      'agent_history': (a) => a['sub'] == 'CssScout'
          ? {
              'items': [
                {'kind': 'user', 'text': 'Find z-index rules'},
                {'kind': 'assistant', 'text': 'nav z-40, sheet z-50'},
              ],
              'messages': <Object>[],
              'title': null,
              'truncated': false,
            }
          : _fixture(),
    });
    await _pump(t, pc, _chat());
    await t.ensureVisible(find.text('CssScout'));
    await t.tap(find.text('CssScout'));
    await _settle(t, 800);

    expect(
      pc.sent('agent_history').any((a) => a['sub'] == 'CssScout'),
      isTrue,
      reason: 'the transcript is asked for by subagent name',
    );
    expect(find.text('omp · subagent'), findsOneWidget);
    expect(find.text('Find z-index rules'), findsWidgets);
    expect(find.byType(TextField), findsNothing, reason: 'no composer');
    expect(find.byTooltip('Stop'), findsNothing);
    await _unmount(t);
  });

  testWidgets('Stop interrupts the working agent; it replaces send', (t) async {
    final pc = _Computer({
      ..._base(
        state: 'working',
        panes: {'p1': _status('omp', 'working', title: 'T')},
      ),
      'agent_interrupt': (_) => <String, Object?>{},
    });
    await _pump(t, pc, _chat(state: 'working'));

    expect(find.byTooltip('Send message'), findsNothing);
    await t.tap(find.byTooltip('Stop'));
    await _settle(t);
    expect(pc.sent('agent_interrupt').single, {'pane': 'p1'});
    await _unmount(t);
  });

  testWidgets('a refused interrupt shows hostd\'s error', (t) async {
    final pc = _Computer({
      ..._base(
        state: 'working',
        panes: {'p1': _status('omp', 'working', title: 'T')},
      ),
      'agent_interrupt': (_) => 'agent is not working',
    });
    await _pump(t, pc, _chat(state: 'working'));
    await t.tap(find.byTooltip('Stop'));
    await _settle(t);
    expect(find.text('agent is not working'), findsOneWidget);
    await _unmount(t);
  });

  testWidgets('while it works the timeline is polled; idle stops the polling', (
    t,
  ) async {
    final pc = _Computer({
      ..._base(
        state: 'working',
        panes: {'p1': _status('omp', 'working', title: 'T')},
      ),
    });
    await _pump(t, pc, _chat(state: 'working'));
    final first = pc.sent('agent_history').length;
    await _settle(t, 4500);
    expect(
      pc.sent('agent_history').length,
      greaterThanOrEqualTo(first + 2),
      reason: 'about every 2 s while working',
    );
    await _unmount(t);
    final after = pc.sent('agent_history').length;
    await _settle(t, 4500);
    expect(pc.sent('agent_history').length, after, reason: 'stops on dispose');
  });

  testWidgets('a finished agent is not polled', (t) async {
    final pc = _Computer(_base());
    await _pump(t, pc, _chat());
    final n = pc.sent('agent_history').length;
    await _settle(t, 5000);
    expect(pc.sent('agent_history').length, n);
    await _unmount(t);
  });

  testWidgets('omp commands are sent as text; /goal asks for its text', (
    t,
  ) async {
    final pc = _Computer({
      ..._base(),
      'agent_send': (_) => <String, Object?>{},
    });
    await _pump(t, pc, _chat());

    await t.tap(find.byTooltip('Commands'));
    await _settle(t);
    await t.tap(find.text('/compact'));
    await _settle(t);
    expect(pc.sent('agent_send').single, {'pane': 'p1', 'text': '/compact'});

    await t.tap(find.byTooltip('Commands'));
    await _settle(t);
    await t.tap(find.text('/goal …'));
    await _settle(t);
    await t.enterText(find.byType(TextField).last, 'ship it');
    await t.tap(find.text('Send'));
    await _settle(t);
    expect(pc.sent('agent_send').last, {'pane': 'p1', 'text': '/goal ship it'});
    await _unmount(t);
  });

  testWidgets('only omp has the commands button', (t) async {
    final pc = _Computer(
      _base(panes: {'p1': _status('claude', 'done', title: 'T')}),
    );
    await _pump(t, pc, _chat(agent: 'claude'));
    expect(find.byTooltip('Commands'), findsNothing);
    expect(find.byTooltip('Send message'), findsOneWidget);
    await _unmount(t);
  });

  testWidgets(
    'the title opens a switcher with the other agents, needs-you first',
    (t) async {
      final pc = _Computer(
        _base(
          panes: {
            'p1': _status('omp', 'done', title: 'Fix the sheet z-index'),
            'p2': _status('claude', 'blocked', title: 'Index orders'),
            'p3': _status('codex', 'working', prompt: 'Refactor the parser'),
          },
        ),
      );
      await _pump(t, pc, _chat());

      await t.tap(find.text('Fix the sheet z-index'));
      await _settle(t);
      expect(find.text('Agents on this computer'), findsOneWidget);
      expect(find.text('Index orders'), findsOneWidget);
      // No title yet: the first prompt stands in.
      expect(find.text('Refactor the parser'), findsOneWidget);

      final blocked = t.getTopLeft(find.text('Index orders')).dy;
      final working = t.getTopLeft(find.text('Refactor the parser')).dy;
      expect(blocked, lessThan(working), reason: 'needs you comes first');

      await t.tap(find.text('Index orders'));
      await _settle(t, 800);
      expect(
        pc.sent('agent_history').any((a) => a['pane'] == 'p2'),
        isTrue,
        reason: 'the chat of the picked agent loads its own history',
      );
      await _unmount(t);
    },
  );

  group('launcher', () {
    Future<void> open(
      WidgetTester t,
      _Computer pc, {
      List<String>? opened,
    }) async {
      await _pump(
        t,
        pc,
        Builder(
          builder: (ctx) => Scaffold(
            body: Center(
              child: FilledButton(
                onPressed: () async {
                  final r = await showAgentLauncher(
                    ctx,
                    device: _mac,
                    projectId: 'proj-1',
                  );
                  if (r != null) opened?.add('${r.pane}:${r.agent}');
                },
                child: const Text('open'),
              ),
            ),
          ),
        ),
      );
      await t.tap(find.text('open'));
      await _settle(t);
    }

    final integrations = {
      'agent_integrations': (_) => {
        'agents': [
          {'id': 'claude', 'label': 'Claude Code', 'state': 'missing'},
          {'id': 'omp', 'label': 'omp', 'state': 'ok'},
          {'id': 'codex', 'label': 'Codex', 'state': 'ok'},
        ],
      },
    };

    testWidgets('sends agent_launch with the project id, agent and prompt', (
      t,
    ) async {
      final opened = <String>[];
      final pc = _Computer({
        ...integrations,
        'agent_launch': (_) => {'pane': 'new-pane', 'pty': 'pty-1'},
      });
      await open(t, pc, opened: opened);

      // Hooks-off agents sort after the ones that report to Atlas, and say so.
      expect(find.text('hooks off'), findsOneWidget);
      // Layout-independent: ok agents sort before the hooks-off one.
      final order =
          [
                for (final c in t.widgetList<Semantics>(find.byType(Semantics)))
                  if (c.properties.button == true && c.properties.label != null)
                    c.properties.label,
              ]
              .whereType<String>()
              .where((l) => ['omp', 'Codex', 'Claude Code'].contains(l));
      expect(order.toList(), ['omp', 'Codex', 'Claude Code']);
      await t.tap(find.text('Codex'));
      await t.pump();
      await t.enterText(find.byType(TextField), 'Refactor the parser');
      await t.tap(find.text('Start agent'));
      await _settle(t);

      expect(pc.sent('agent_launch').single, {
        'project': 'proj-1',
        'agent': 'codex',
        'prompt': 'Refactor the parser',
      });
      expect(opened, ['new-pane:codex']);
      await _unmount(t);
    });

    testWidgets(
      'no prompt: the key is left out; first ok agent is preselected',
      (t) async {
        final pc = _Computer({
          ...integrations,
          'agent_launch': (_) => {'pane': 'p', 'pty': 'x'},
        });
        await open(t, pc);
        await t.tap(find.text('Start agent'));
        await _settle(t);
        expect(pc.sent('agent_launch').single, {
          'project': 'proj-1',
          'agent': 'omp',
        });
        await _unmount(t);
      },
    );

    testWidgets('an error stays inline and the sheet stays open', (t) async {
      final opened = <String>[];
      final pc = _Computer({
        ...integrations,
        'agent_launch': (_) => 'Atlas Desktop is not running',
      });
      await open(t, pc, opened: opened);
      await t.tap(find.text('Start agent'));
      await _settle(t);
      expect(find.text('Atlas Desktop is not running'), findsOneWidget);
      expect(find.text('Start agent'), findsOneWidget);
      expect(opened, isEmpty);
      await _unmount(t);
    });

    testWidgets('no installed agent says so', (t) async {
      final pc = _Computer({
        'agent_integrations': (_) => {'agents': <Object>[]},
      });
      await open(t, pc);
      expect(find.textContaining('No agent found'), findsOneWidget);
      await _unmount(t);
    });
  });

  group('display helpers', () {
    test(
      'title: session title, else first prompt trimmed, else agent name',
      () {
        AgentStatus s({String? title, String? prompt}) => AgentStatus.fromJson(
          _status('claude', 'idle', title: title, prompt: prompt),
        );
        expect(agentTitle(s(title: 'Named', prompt: 'p')), 'Named');
        expect(agentTitle(s(prompt: '  Fix\n  the   thing ')), 'Fix the thing');
        expect(agentTitle(s(prompt: 'x' * 90)), '${'x' * 60}…');
        expect(agentTitle(s()), 'Claude Code');
        // The history's own title beats the pushed one (it is newer).
        expect(agentTitle(s(title: 'old'), historyTitle: 'new'), 'new');
        expect(agentTitle(s(), firstPrompt: 'from history'), 'from history');
      },
    );

    test('elapsed reads like a stopwatch and hides a missing start', () {
      const now = 10 * 3600 * 1000;
      expect(elapsed(now - 5000, nowMs: now), '5s');
      expect(elapsed(now - 4 * 60000, nowMs: now), '4m');
      expect(elapsed(now - (2 * 3600 + 5 * 60) * 1000, nowMs: now), '2h 5m');
      expect(elapsed(now - 3 * 3600 * 1000, nowMs: now), '3h');
      expect(elapsed(0, nowMs: now), '');
      expect(agentLabel('mystery'), 'mystery');
    });
  });

  group('project agents tab', () {
    final detail = jsonDecode(
      File('test/fixtures/project_detail.json').readAsStringSync(),
    ) as Map<String, dynamic>;
    final project = ProjectInfo.fromJson({
      'id': 'a',
      'root': '/Users/me/shop',
      'name': 'shop',
      'running': false,
      'services': <Object>[],
      'agents': <Object>[],
    });

    Map<String, Object? Function(Map<String, dynamic>)> replies({
      String? title,
    }) => {
      ..._base(panes: {'p-in': _status('omp', 'done', title: title)}),
      'project_detail': (_) => {
        ...detail,
        'agents': [
          {
            'pane_key': 'p-in',
            'status': {
              ...(detail['agents'] as List).first['status'] as Map,
              'title': title,
            },
          },
        ],
      },
      'agent_integrations': (_) => {
        'agents': [
          {'id': 'omp', 'label': 'omp', 'state': 'ok'},
          {'id': 'claude', 'label': 'Claude Code', 'state': 'ok'},
        ],
      },
      'agent_launch': (_) => {'pane': 'fresh', 'pty': 'pty-fresh'},
    };

    Future<void> openTab(WidgetTester t, _Computer pc) async {
      await _pump(t, pc, ProjectDetailScreen(device: _mac, project: project));
      await t.tap(find.text('Agents 1'));
      await _settle(t);
    }

    testWidgets('rows show logo, session title, state and a New agent action', (
      t,
    ) async {
      final pc = _Computer(replies(title: 'Say hi to the user'));
      await openTab(t, pc);
      expect(find.text('Say hi to the user'), findsOneWidget);
      expect(find.byType(AgentLogo), findsWidgets);
      expect(find.textContaining('done'), findsWidgets);
      expect(find.text('New agent'), findsOneWidget);
      await _unmount(t);
    });

    testWidgets('a row without a title shows the first prompt', (t) async {
      final pc = _Computer(replies());
      await openTab(t, pc);
      // The fixture's prompt is "diga oi".
      expect(find.text('diga oi'), findsWidgets);
      await _unmount(t);
    });

    testWidgets('New agent launches in this project and opens its chat', (
      t,
    ) async {
      final pc = _Computer(replies());
      await openTab(t, pc);
      await t.tap(find.text('New agent'));
      await _settle(t);
      await t.tap(find.text('Claude Code'));
      await t.pump();
      await t.enterText(find.byType(TextField), 'Audit the auth flow');
      await t.tap(find.text('Start agent'));
      await _settle(t, 1000);

      expect(pc.sent('agent_launch').single, {
        'project': 'a',
        'agent': 'claude',
        'prompt': 'Audit the auth flow',
      });
      // The chat of the returned pane is open and asked for its own history.
      expect(find.byType(AgentChatScreen), findsOneWidget);
      expect(pc.sent('agent_history').any((a) => a['pane'] == 'fresh'), isTrue);
      await _unmount(t);
    });
  });
}
