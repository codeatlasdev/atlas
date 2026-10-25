import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_hub_screen.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/crew_widgets.dart';
import 'package:atlas_mobile/features/agents/timeline_widgets.dart';

/// The real hostd output: an omp session with Scout (running, spawned Deep), Fixer (failed).
AgentTree _tree() => AgentTree.fromJson(
  jsonDecode(File('test/fixtures/agent_tree.json').readAsStringSync())
      as Map<String, dynamic>,
);

/// Running agents animate forever, so `pumpAndSettle` never returns: pump a fixed time.
Future<void> _settle(WidgetTester t, [int ms = 500]) async {
  for (var i = 0; i < ms ~/ 50; i++) {
    await t.pump(const Duration(milliseconds: 50));
  }
}

/// Scrolls the open detail sheet until [key] is on screen, then taps it.
Future<void> _tapInSheet(WidgetTester t, Key key) async {
  await t.ensureVisible(find.byKey(key));
  await _settle(t, 100);
  await t.tap(find.byKey(key));
  // A sheet closes and the next one opens: both transitions.
  await _settle(t, 1200);
}

/// What the callbacks of a screen received, in order.
class _Calls {
  final focused = <String>[];
  final messaged = <String>[];
}

Future<_Calls> _hub(
  WidgetTester t, {
  AgentTree? tree,
  HubLayout layout = HubLayout.byParent,
  String focus = '',
  bool canMessage = true,
}) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  addTearDown(() => t.binding.setSurfaceSize(null));
  final calls = _Calls();
  final notifier = ValueNotifier<AgentTree>(tree ?? _tree());
  addTearDown(notifier.dispose);
  await t.pumpWidget(
    MaterialApp(
      theme: atlasTheme(),
      home: Builder(
        builder: (context) => Scaffold(
          body: Center(
            child: ElevatedButton(
              onPressed: () => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => AgentHubScreen(
                    tree: notifier,
                    focus: focus,
                    fallbackAgent: 'omp',
                    initialLayout: layout,
                    onFocus: calls.focused.add,
                    onMessage: canMessage
                        ? (n) => calls.messaged.add(n.id)
                        : null,
                  ),
                ),
              ),
              child: const Text('open'),
            ),
          ),
        ),
      ),
    ),
  );
  await t.tap(find.text('open'));
  await _settle(t);
  return calls;
}

List<String> _rowOrder(WidgetTester t) {
  final rows = find.byType(HubRow).evaluate().map((e) => e.widget as HubRow);
  return [for (final r in rows) r.node.id];
}

void main() {
  group('hub roster', () {
    testWidgets('header counts running and parked, with the totals', (t) async {
      await _hub(t);
      // Root + Scout run; Fixer failed, Deep done.
      expect(find.text('2 running · 2 parked'), findsOneWidget);
      final totals = t.widget<Text>(find.byKey(const ValueKey('hub-totals')));
      // 0.04 + 0.04 ... summed over the four nodes; active 9s (Scout only).
      expect(totals.data, contains('9s active'));
      expect(totals.data, contains('req'));
      expect(totals.data, contains('tools'));
      expect(totals.data, contains('tok'));
      expect(totals.data, startsWith(r'$'));
    });

    testWidgets('by parent: a tree in spawn order with indentation', (t) async {
      await _hub(t, layout: HubLayout.byParent);
      expect(_rowOrder(t), ['', 'Fixer', 'Scout', 'Scout.Deep']);
      double left(String id) => t
          .getTopLeft(
            find
                .descendant(
                  of: find.byKey(ValueKey('hub-$id')),
                  matching: find.byType(Material),
                )
                .first,
          )
          .dx;
      expect(left('Fixer'), greaterThan(left('')));
      expect(left('Scout.Deep'), greaterThan(left('Scout')));
      expect(
        find.textContaining('↳'),
        findsNothing,
        reason: 'indentation says it',
      );
    });

    testWidgets('flat: one level, each row names its parent', (t) async {
      await _hub(t, layout: HubLayout.flat);
      double left(String id) => t
          .getTopLeft(
            find
                .descendant(
                  of: find.byKey(ValueKey('hub-$id')),
                  matching: find.byType(Material),
                )
                .first,
          )
          .dx;
      expect(left('Scout.Deep'), left(''));
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('hub-Scout.Deep')),
          matching: find.text('↳ Scout'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('hub-Scout')),
          matching: find.text('↳ Main'),
        ),
        findsOneWidget,
      );
    });

    testWidgets('the segmented control switches layout', (t) async {
      await _hub(t, layout: HubLayout.byParent);
      double left() => t
          .getTopLeft(
            find
                .descendant(
                  of: find.byKey(const ValueKey('hub-Scout.Deep')),
                  matching: find.byType(Material),
                )
                .first,
          )
          .dx;
      final nested = left();
      await t.tap(find.text('Flat'));
      await _settle(t);
      expect(left(), lessThan(nested));
      await t.tap(find.text('By parent'));
      await _settle(t);
      expect(left(), nested);
    });

    testWidgets(
      'a row shows type · model · thinking, the task and the numbers',
      (t) async {
        await _hub(t);
        final scout = find.byKey(const ValueKey('hub-Scout'));
        expect(
          find.descendant(
            of: scout,
            matching: find.text('SCOUT · claude-sonnet-5-5 · low'),
          ),
          findsOneWidget,
        );
        expect(
          find.descendant(of: scout, matching: find.text('Map hostd')),
          findsOneWidget,
        );
        expect(
          find.descendant(of: scout, matching: find.textContaining('2 req')),
          findsOneWidget,
        );
        // The main agent: its title is the task line.
        final root = find.byKey(const ValueKey('hub-'));
        expect(
          find.descendant(of: root, matching: find.text('Main')),
          findsOneWidget,
        );
        expect(
          find.descendant(of: root, matching: find.text('Map the host')),
          findsOneWidget,
        );
      },
    );

    testWidgets('filters narrow the roster and say how many', (t) async {
      await _hub(t);
      expect(find.text('All 4'), findsOneWidget);
      expect(find.text('Running 2'), findsOneWidget);
      expect(find.text('Done 1'), findsOneWidget);
      expect(find.text('Failed 1'), findsOneWidget);

      await t.tap(find.byKey(const ValueKey('hub-filter-failed')));
      await _settle(t);
      expect(_rowOrder(t), ['Fixer']);

      await t.tap(find.byKey(const ValueKey('hub-filter-done')));
      await _settle(t);
      expect(_rowOrder(t), ['Scout.Deep']);

      await t.tap(find.byKey(const ValueKey('hub-filter-running')));
      await _settle(t);
      expect(_rowOrder(t), ['', 'Scout']);

      await t.tap(find.byKey(const ValueKey('hub-filter-all')));
      await _settle(t);
      expect(_rowOrder(t).length, 4);
    });

    testWidgets('a filter with no match says so; an empty tree too', (t) async {
      await _hub(
        t,
        tree: const AgentTree([AgentNode(id: '', status: 'idle')]),
      );
      await t.tap(find.byKey(const ValueKey('hub-filter-failed')));
      await _settle(t);
      expect(find.text('No agents match this filter'), findsOneWidget);
    });
  });

  group('agent detail', () {
    testWidgets(
      'shows Task, Current, Usage with the context bar, Lineage, Recent',
      (t) async {
        await _hub(t);
        await t.tap(find.byKey(const ValueKey('hub-Scout')));
        await _settle(t);

        expect(find.text('Task'), findsOneWidget);
        expect(find.text('Map hostd'), findsWidgets);
        expect(find.text('Current'), findsOneWidget);
        expect(find.text('Reading server.rs'), findsWidgets);
        expect(find.text('Usage'), findsOneWidget);
        final usage = t.widget<Text>(
          find.byKey(const ValueKey('detail-usage')),
        );
        expect(usage.data, contains('9s active'));
        expect(usage.data, contains('2 req'));
        expect(find.text('context 292K/1M  29%'), findsOneWidget);
        final bar = t.widget<LinearProgressIndicator>(
          find.byKey(const ValueKey('context-bar')),
        );
        expect(bar.value, closeTo(.292, 1e-9));
        expect(find.text('Lineage'), findsOneWidget);
        expect(find.text('spawned by'), findsOneWidget);
        expect(find.text('1 child'), findsOneWidget);
        expect(find.text('Recent activity'), findsOneWidget);
        // Newest first, one row per call with its outcome icon and tool name.
        final recent = find.byKey(const ValueKey('detail-recent'));
        Finder tool(String name) => find.descendant(
          of: recent,
          matching: find.byWidgetPredicate(
            (w) => w is RichText && w.text.toPlainText().startsWith(name),
          ),
        );
        expect(tool('read'), findsOneWidget);
        expect(tool('task'), findsOneWidget);
        expect(
          find.descendant(
            of: recent,
            matching: find.byIcon(Icons.check_rounded),
          ),
          findsOneWidget,
        );
        expect(
          find.descendant(
            of: recent,
            matching: find.byIcon(Icons.autorenew_rounded),
          ),
          findsOneWidget,
        );
        expect(
          t.getTopLeft(tool('read')).dy,
          lessThan(t.getTopLeft(tool('task')).dy),
          reason: 'newest (read, last in the list) on top',
        );
      },
    );

    testWidgets('a node without context numbers has no bar', (t) async {
      await _hub(t);
      await t.tap(find.byKey(const ValueKey('hub-Fixer')));
      await _settle(t);
      expect(find.byKey(const ValueKey('context-bar')), findsNothing);
      expect(find.text('Current'), findsNothing, reason: 'it is not running');
    });

    testWidgets('lineage jumps to the parent and to a child', (t) async {
      await _hub(t);
      await t.tap(find.byKey(const ValueKey('hub-Scout')));
      await _settle(t);
      expect(find.text('1 child'), findsOneWidget);

      await _tapInSheet(t, const ValueKey('lineage-child-Scout.Deep'));
      // Deep's detail now: its task, spawned by Scout.
      expect(find.text('Read server.rs'), findsWidgets);
      expect(find.text('spawned by'), findsOneWidget);
      expect(
        find.byKey(const ValueKey('lineage-child-Scout.Deep')),
        findsNothing,
      );

      await _tapInSheet(t, const ValueKey('lineage-parent'));
      expect(find.text('Map hostd'), findsWidgets, reason: 'back on Scout');
      expect(find.text('1 child'), findsOneWidget);

      // Scout's parent is the main agent, named `Main`.
      await _tapInSheet(t, const ValueKey('lineage-parent'));
      expect(find.text('Todos'), findsOneWidget, reason: 'the root detail');
      expect(find.text('2 children'), findsOneWidget);
    });

    testWidgets('Open conversation focuses the agent and leaves the hub', (
      t,
    ) async {
      final calls = await _hub(t);
      await t.tap(find.byKey(const ValueKey('hub-Scout.Deep')));
      await _settle(t);
      await t.tap(find.byKey(const ValueKey('detail-open')));
      await _settle(t, 1200);
      expect(calls.focused, ['Scout.Deep']);
      expect(find.text('Agent hub'), findsNothing);
    });

    testWidgets('Back to parent opens the parent; the root says Main', (
      t,
    ) async {
      final calls = await _hub(t);
      await t.tap(find.byKey(const ValueKey('hub-Scout.Deep')));
      await _settle(t);
      expect(find.text('Back to Scout'), findsOneWidget);
      await t.tap(find.byKey(const ValueKey('detail-parent')));
      await _settle(t);
      expect(calls.focused, ['Scout']);
    });

    testWidgets('a depth-1 agent goes back to Main', (t) async {
      final calls = await _hub(t);
      await t.tap(find.byKey(const ValueKey('hub-Fixer')));
      await _settle(t);
      expect(find.text('Back to Main'), findsOneWidget);
      await t.tap(find.byKey(const ValueKey('detail-parent')));
      await _settle(t);
      expect(calls.focused, ['']);
    });

    testWidgets('Message is labelled for who it reaches', (t) async {
      final calls = await _hub(t);
      await t.tap(find.byKey(const ValueKey('hub-Scout')));
      await _settle(t);
      expect(find.text('Message Main about Scout'), findsOneWidget);
      await t.tap(find.byKey(const ValueKey('detail-message')));
      await _settle(t, 1200);
      expect(calls.messaged, ['Scout']);
      expect(find.text('Agent hub'), findsNothing);
    });

    testWidgets('the root detail has Message, no parent, and its todos', (
      t,
    ) async {
      await _hub(t);
      await t.tap(find.byKey(const ValueKey('hub-')));
      await _settle(t);
      expect(find.text('Message'), findsOneWidget);
      expect(find.byKey(const ValueKey('detail-parent')), findsNothing);
      expect(find.text('Todos'), findsOneWidget);
      expect(find.text('1/3 done · Write the hub'), findsOneWidget);
      expect(
        find.text('2 children'),
        findsOneWidget,
        reason: 'Fixer and Scout',
      );
    });

    testWidgets('no Message button when the agent cannot relay', (t) async {
      await _hub(t, canMessage: false);
      await t.tap(find.byKey(const ValueKey('hub-Scout')));
      await _settle(t);
      expect(find.byKey(const ValueKey('detail-message')), findsNothing);
      expect(find.byKey(const ValueKey('detail-open')), findsOneWidget);
    });
  });

  group('root strips', () {
    testWidgets('waiting on N jobs lists the running subagents', (t) async {
      await t.pumpWidget(
        MaterialApp(
          theme: atlasTheme(),
          home: Scaffold(
            body: JobsStrip(tree: _tree(), onOpen: (_) {}),
          ),
        ),
      );
      expect(find.text('Waiting on 1 job'), findsOneWidget);
      expect(find.byKey(const ValueKey('job-Scout')), findsOneWidget);
      expect(find.byKey(const ValueKey('job-Fixer')), findsNothing);
      // The bar is the context share of that job.
      final bar = t.widget<LinearProgressIndicator>(
        find.descendant(
          of: find.byKey(const ValueKey('job-Scout')),
          matching: find.byType(LinearProgressIndicator),
        ),
      );
      expect(bar.value, closeTo(.292, 1e-9));
    });

    testWidgets('nothing running: no strip', (t) async {
      await t.pumpWidget(
        MaterialApp(
          theme: atlasTheme(),
          home: Scaffold(
            body: JobsStrip(
              tree: const AgentTree([AgentNode(id: '', status: 'idle')]),
              onOpen: (_) {},
            ),
          ),
        ),
      );
      expect(find.textContaining('Waiting on'), findsNothing);
    });

    testWidgets('the todo card collapses to the current task and expands', (
      t,
    ) async {
      final todos = _tree().node('')!.todos!;
      await t.pumpWidget(
        MaterialApp(
          theme: atlasTheme(),
          home: Scaffold(body: TodoCard(todos)),
        ),
      );
      expect(find.text('1/3'), findsOneWidget);
      expect(find.text('Write the hub'), findsOneWidget);
      expect(find.text('Read hostd'), findsNothing, reason: 'collapsed');
      await t.tap(find.byKey(const ValueKey('todo-toggle')));
      await _settle(t);
      expect(find.text('Map'), findsOneWidget);
      expect(find.text('Ship'), findsOneWidget);
      expect(find.text('Read hostd'), findsOneWidget);
      expect(find.byIcon(Icons.check_box_rounded), findsOneWidget);
      expect(find.byIcon(Icons.timelapse_rounded), findsOneWidget);
      expect(
        find.byIcon(Icons.check_box_outline_blank_rounded),
        findsOneWidget,
      );
    });
  });

  group('irc, notice and advisor cards', () {
    Future<void> show(
      WidgetTester t,
      TimelineItem item, {
      void Function(String)? open,
      String? parentId,
    }) => t.pumpWidget(
      MaterialApp(
        theme: atlasTheme(),
        home: Scaffold(
          body: Builder(
            builder: (context) => timelineBlock(
              context,
              ItemBlock(item),
              history: const AgentHistory(),
              onOpenSubagent: open ?? (_) {},
              parentId: parentId,
            ),
          ),
        ),
      ),
    );

    testWidgets('an IRC message is a card from its sender, not a user bubble', (
      t,
    ) async {
      final opened = <String>[];
      await show(
        t,
        const IrcItem(
          from: 'parent',
          agent: 'Main',
          text: 'FYI: use the label',
        ),
        open: opened.add,
        parentId: 'Scout',
      );
      expect(find.byType(UserBubble), findsNothing);
      expect(find.text('Message from Main (parent)'), findsOneWidget);
      expect(find.text('FYI: use the label'), findsOneWidget);
      // The parent is the focused agent's own parent, which may be a subagent.
      await t.tap(find.byType(IrcCard));
      expect(opened, ['Scout']);
    });

    testWidgets('an IRC message from a peer is not a link', (t) async {
      await show(t, const IrcItem(from: 'Scout', agent: 'Scout', text: 'pong'));
      expect(find.text('Message from Scout'), findsOneWidget);
      expect(find.byIcon(Icons.chevron_right), findsNothing);
    });

    testWidgets('a notice is one muted line that expands', (t) async {
      await show(t, const NoticeItem('Job X done.\nResume your work.'));
      expect(find.text('Job X done.'), findsOneWidget);
      expect(find.textContaining('Resume'), findsNothing);
      await t.tap(find.byType(NoticeRow));
      await t.pump();
      expect(find.textContaining('Resume your work.'), findsOneWidget);
    });

    testWidgets('an advisor note takes its colour from the severity', (
      t,
    ) async {
      Color? border() {
        final box = t.widget<Container>(
          find
              .descendant(
                of: find.byType(AdvisorCard),
                matching: find.byType(Container),
              )
              .first,
        );
        return ((box.decoration! as BoxDecoration).border! as Border)
            .left
            .color;
      }

      await show(t, const AdvisorItem(severity: 'blocker', text: 'Ask first'));
      final c = AtlasColors.of(t.element(find.byType(AdvisorCard)));
      expect(find.text('Advisor · blocker'), findsOneWidget);
      expect(border(), c.error);

      await show(t, const AdvisorItem(severity: 'concern', text: 'Check it'));
      expect(border(), c.warning);

      await show(t, const AdvisorItem(severity: 'nit', text: 'Naming'));
      expect(border(), c.muted);
    });
  });

  group('title', () {
    AgentStatus st({String? title, String? prompt}) => AgentStatus.fromJson({
      'state': 'done',
      'agent': 'omp',
      'updated_at': 1,
      'state_started_at': 1,
      'title': title,
      'prompt': prompt,
    });

    test(
      'history title, status title, first prompt, hook prompt, agent name',
      () {
        expect(
          agentTitle(st(title: 'Status'), historyTitle: 'From hostd'),
          'From hostd',
        );
        expect(agentTitle(st(title: 'Status')), 'Status');
        expect(
          agentTitle(st(), firstPrompt: 'Corrige o sheet\nmais'),
          'Corrige o sheet mais',
        );
        expect(agentTitle(st(prompt: 'from the hook')), 'from the hook');
        expect(agentTitle(st()), 'omp');
      },
    );

    test('an IRC or notice is never taken as the first prompt', () {
      const h = AgentHistory(
        items: [
          NoticeItem('boot'),
          IrcItem(from: 'parent', agent: 'Main', text: 'Complete assignment…'),
          UserItem('the real prompt'),
        ],
      );
      final firstUser = h.items.whereType<UserItem>().first.text;
      expect(firstUser, 'the real prompt');
    });
  });

  group('formatters', () {
    test('context windows, ages and clocks', () {
      expect(windowText(292000), '292K');
      expect(windowText(1000000), '1M');
      expect(windowText(1500000), '1.5M');
      expect(windowText(812), '812');
      expect(agoText(0), '');
      expect(agoText(10000, nowMs: 12000), 'just now');
      expect(agoText(10000, nowMs: 40000), '30s ago');
      expect(agoText(0 + 1, nowMs: 5 * 60 * 1000 + 1), '5m ago');
      expect(agoText(100, nowMs: 50), '', reason: 'never in the future');
      expect(clockText(0), '');
      expect(
        clockText(DateTime(2026, 1, 2, 3, 4, 5).millisecondsSinceEpoch),
        '03:04:05',
      );
    });

    test('totals add the figures that exist and skip the rest', () {
      expect(HubTotals.of(const []).line, '');
      final t = HubTotals.of(const [
        AgentNode(
          id: 'a',
          status: 'done',
          cost: .5,
          requests: 2,
          toolCount: 3,
          tokens: 1500,
          activeMs: 61000,
        ),
        AgentNode(id: 'b', status: 'done', cost: .25, requests: 1, tokens: 500),
      ]);
      expect(t.line, r'$0.75 · 1m 1s active · 3 req · 3 tools · 2.0k tok');
    });
  });
}
