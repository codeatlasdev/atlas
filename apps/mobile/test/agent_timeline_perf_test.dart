import 'dart:async';
import 'dart:convert';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/agents/timeline/chat_messages.dart';
import 'package:atlas_mobile/features/agents/tree/crew_rail.dart';

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// A computer that answers by command; the reply of a command is read at call time, so
/// a test can change what hostd says between polls.
class _Computer implements Transport {
  _Computer(this.replies);
  final Map<String, Object? Function(Map<String, dynamic> args)> replies;
  final calls = <String>[];
  final _lines = StreamController<String>.broadcast();

  @override
  Stream<String> get incomingLines => _lines.stream;

  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    final cmd = req['cmd'] as String;
    final args = (req['args'] as Map?)?.cast<String, dynamic>() ?? const {};
    calls.add(cmd);
    final r =
        replies[cmd]?.call(args) ??
        (cmd == 'info' ? {'proto': kAppProto} : <String, Object?>{});
    scheduleMicrotask(
      () => _lines.add(jsonEncode({'id': req['id'], 'ok': true, 'data': r})),
    );
  }

  @override
  Future<void> close() async => _lines.close();

  int count(String cmd) => calls.where((c) => c == cmd).length;

  /// A hostd push (`agent_status`, ...): a line with `ev` and no `id`.
  void push(Map<String, Object?> event) => _lines.add(jsonEncode(event));
}

const _mac = MacDevice(nodeId: 'n1', name: 'My Mac');

/// [n] items: alternating user and assistant turns, so the list has n blocks.
Map<String, Object?> _history(int n, {String? lastText}) => {
  'title': 'Long chat',
  'items': [
    for (var i = 0; i < n; i++)
      if (i.isEven)
        {'kind': 'user', 'text': 'question $i'}
      else
        {
          'kind': 'assistant',
          'text': lastText != null && i == n - 1 ? lastText : 'answer $i',
        },
  ],
};

Map<String, Object?> _status(String state, {int updatedAt = 1000}) => {
  'state': state,
  'agent': 'omp',
  'model': 'gpt-5.5',
  'prompt': null,
  'updated_at': updatedAt,
  'state_started_at': DateTime.now().millisecondsSinceEpoch,
  'title': 'T',
};

Future<void> _pump(WidgetTester t, _Computer pc) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  addTearDown(() => t.binding.setSurfaceSize(null));
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage()),
        transportFactoryProvider.overrideWithValue((_, _) async => pc),
      ],
      child: MaterialApp(
        theme: atlasTheme(),
        home: AgentChatScreen(
          device: _mac,
          paneKey: 'p1',
          initial: AgentStatus.fromJson(_status('working')),
          title: 'agent',
        ),
      ),
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

/// The widgets the timeline mounts for its blocks.
Iterable<Widget> _blocks(WidgetTester t) => [
  ...t.widgetList(find.byType(UserBubble)),
  ...t.widgetList(find.byType(AssistantText)),
];

Map<String, Object? Function(Map<String, dynamic>)> _replies(
  Object? Function() history, {
  Object? Function()? tree,
}) => {
  'agent_status_all': (_) => {
    'panes': {'p1': _status('working')},
    'hook_port': 0,
    'pending_approvals': <Object>[],
  },
  'agent_history': (_) => history(),
  'agent_tree': (_) => tree?.call() ?? {'nodes': <Object>[]},
};

void main() {
  testWidgets('an unchanged poll does not rebuild the timeline', (t) async {
    final h = _history(200);
    final pc = _Computer(_replies(() => h));
    await _pump(t, pc);
    final before = _blocks(t).toList();
    expect(before, isNotEmpty);
    final polls = pc.count('agent_history');

    // Three more polls, each answering the very same history.
    await _settle(t, 6500);
    expect(
      pc.count('agent_history'),
      greaterThanOrEqualTo(polls + 3),
      reason: 'the polling really ran',
    );

    final after = _blocks(t).toList();
    expect(after.length, before.length);
    for (var i = 0; i < before.length; i++) {
      expect(
        identical(after[i], before[i]),
        isTrue,
        reason: 'block $i was rebuilt by a poll that changed nothing',
      );
    }
    await _unmount(t);
  });

  testWidgets('an unchanged agent_tree poll does not rebuild the screen', (
    t,
  ) async {
    final pc = _Computer(
      _replies(
        () => _history(40),
        tree: () => {
          'nodes': [
            {'id': '', 'status': 'running'},
            {'id': 'Scout', 'parent': '', 'name': 'Scout', 'status': 'running'},
          ],
        },
      ),
    );
    await _pump(t, pc);
    // CrewRail is made fresh by every build of the screen.
    final rail = t.widget<CrewRail>(find.byType(CrewRail));
    final blocks = _blocks(t).toList();
    final polls = pc.count('agent_tree');
    await _settle(t, 6500);
    expect(pc.count('agent_tree'), greaterThanOrEqualTo(polls + 3));
    expect(
      identical(t.widget<CrewRail>(find.byType(CrewRail)), rail),
      isTrue,
      reason: 'the screen was rebuilt by a crew poll that changed nothing',
    );
    final after = _blocks(t).toList();
    for (var i = 0; i < blocks.length; i++) {
      expect(identical(after[i], blocks[i]), isTrue, reason: 'block $i');
    }
    await _unmount(t);
  });

  testWidgets('a changed crew still updates the screen', (t) async {
    var running = 'running';
    final pc = _Computer(
      _replies(
        () => _history(6),
        tree: () => {
          'nodes': [
            {'id': '', 'status': 'running'},
            {'id': 'Scout', 'parent': '', 'name': 'Scout', 'status': running},
          ],
        },
      ),
    );
    await _pump(t, pc);
    expect(find.byType(CrewRail), findsOneWidget);
    final rail = t.widget<CrewRail>(find.byType(CrewRail));
    expect(rail.tree.node('Scout')!.status, 'running');
    running = 'completed';
    await _settle(t, 2500);
    final now = t.widget<CrewRail>(find.byType(CrewRail));
    expect(now.tree.node('Scout')!.status, 'completed');
    await _unmount(t);
  });

  testWidgets('a rebuild for another reason reuses the built blocks', (
    t,
  ) async {
    final pc = _Computer(_replies(() => _history(200)));
    await _pump(t, pc);
    final before = _blocks(t).toList();
    // A status push rebuilds the screen (its state, its composer) and nothing in the
    // history: the blocks on screen must be the very same widgets afterwards.
    for (var i = 1; i <= 3; i++) {
      pc.push({
        'ev': 'agent_status',
        'pane_key': 'p1',
        'status': _status('working', updatedAt: 1000 + i),
      });
      await _settle(t, 100);
    }
    final after = _blocks(t).toList();
    expect(after.length, before.length);
    for (var i = 0; i < before.length; i++) {
      expect(
        identical(after[i], before[i]),
        isTrue,
        reason: 'block $i was rebuilt by a rebuild of the screen alone',
      );
    }
    await _unmount(t);
  });

  testWidgets('200 items: only the blocks near the viewport are built', (
    t,
  ) async {
    final pc = _Computer(_replies(() => _history(200)));
    await _pump(t, pc);
    final built = _blocks(t).length;
    expect(built, greaterThan(0));
    expect(
      built,
      lessThan(40),
      reason:
          'a 852 dp viewport must not lay out all 200 blocks (built $built)',
    );
    // The newest is at the bottom, and older ones come in as the user scrolls up.
    expect(find.text('answer 199'), findsOneWidget);
    expect(find.text('question 0'), findsNothing);
    await t.drag(find.byType(Scrollable).last, const Offset(0, 6000));
    await _settle(t, 200);
    expect(
      _blocks(t).length,
      lessThan(40),
      reason: 'still bounded after scrolling',
    );
    await _unmount(t);
  });

  // Numbers, not a pass/fail on speed: the wall time of N unchanged polls on a 200-item
  // chat, and how many block widgets are mounted. Printed so a regression is visible in
  // the log; the assertions above are what gate it.
  testWidgets('benchmark: 200 items, 5 unchanged polls', (t) async {
    final pc = _Computer(_replies(() => _history(200)));
    await _pump(t, pc);
    final sw = Stopwatch()..start();
    var frames = 0;
    for (var i = 0; i < 5 * 40; i++) {
      await t.pump(const Duration(milliseconds: 50));
      frames++;
    }
    sw.stop();
    // ignore: avoid_print
    print(
      'timeline-bench: ${_blocks(t).length} blocks mounted of 200, '
      '$frames frames over 5 polls took ${sw.elapsedMilliseconds} ms '
      '(${(sw.elapsedMicroseconds / frames).round()} us/frame)',
    );
    await _unmount(t);
  });
}
