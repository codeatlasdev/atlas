import 'dart:convert';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/auth/device_auth.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/crew/chat/action_card.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';

import 'support/demo_harness.dart';

// Regression: with the computer off, tapping Approve / Deny / an answer / Cancel / Send did NOTHING and said
// nothing (the handler did `if (mux == null) return`), so the user believed they had answered. Every action the
// user triggers goes through `requireMuxOf` and shows the shared offline sentence where it lives; what they typed
// stays, and nothing is marked sent. The online path still sends exactly one call.

const _mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');
const _offline = 'Demo MacBook is offline';

/// The demo computer's transport, recording every command the app sends.
class _Recording implements Transport {
  _Recording(this.inner);
  final Transport inner;
  final sent = <Map<String, dynamic>>[];
  @override
  Stream<String> get incomingLines => inner.incomingLines;
  @override
  Future<void> sendLine(String line) {
    sent.add(jsonDecode(line) as Map<String, dynamic>);
    return inner.sendLine(line);
  }

  @override
  Future<void> close() => inner.close();

  List<Map<String, dynamic>> calls(String cmd) => [
    for (final m in sent)
      if (m['cmd'] == cmd) m,
  ];
}

/// A computer that is on, then goes off: the link closes and every redial is refused.
class _Link {
  bool off = false;
  final computer = <_Recording>[];

  /// Everything the app sent to any link of this computer.
  List<Map<String, dynamic>> calls(String cmd) => [
    for (final r in computer) ...r.calls(cmd),
  ];

  Future<Transport> dial() async {
    if (off) throw Exception('refused');
    final r = _Recording(demoTransport());
    computer.add(r);
    return r;
  }

  /// The computer goes off.
  Future<void> goOffline(WidgetTester t) async {
    off = true;
    for (final r in computer) {
      await r.close();
    }
    // The reconnect backoff (1 s) fires and the redial is refused.
    for (var i = 0; i < 30; i++) {
      await t.pump(const Duration(milliseconds: 100));
    }
  }
}

Future<_Link> _pump(
  WidgetTester t,
  Widget screen, {
  List<Override> overrides = const [],
}) async {
  final link = _Link();
  await t.binding.setSurfaceSize(const Size(393, 852));
  addTearDown(() => t.binding.setSurfaceSize(null));
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage(const [_mac])),
        transportFactoryProvider.overrideWithValue((_, _) => link.dial()),
        ...overrides,
      ],
      child: MaterialApp(theme: atlasTheme(), home: screen),
    ),
  );
  await settle(t);
  return link;
}

/// The chat keeps its device's connection open while a card is on screen; so does this.
Widget _withConnection(Widget child) => Consumer(
  builder: (context, ref, _) {
    ref.watch(connectionNotifierProvider(_mac));
    return child;
  },
);

CrewmateTask _task(Map<String, dynamic> action) => CrewmateTask.fromJson({
  'id': 'task-1',
  'ts': 1700000000,
  'source': 'mobile',
  'prompt': 'deploy',
  'state': 'waiting_action',
  'action': action,
});

Widget _card(Map<String, dynamic> action) => Scaffold(
  body: SingleChildScrollView(
    child: _withConnection(
      TaskActionCard(
        task: _task(action),
        device: _mac,
        crewmateId: 'crewmate-1',
      ),
    ),
  ),
);

Future<void> _unmount(WidgetTester t) async {
  await t.pumpWidget(const SizedBox());
  await t.pump(const Duration(seconds: 1));
}

void main() {
  group('action card', () {
    final input = {'kind': 'input', 'label': 'Branch', 'placeholder': 'name'};

    testWidgets(
      'Submit offline says so, keeps the typed answer, sends nothing',
      (t) async {
        final link = await _pump(t, _card(input));
        await link.goOffline(t);

        await t.enterText(find.byType(TextField), 'release-2');
        await t.tap(find.text('Submit'));
        await settle(t);

        expect(find.text(_offline), findsOneWidget);
        expect(
          t.widget<TextField>(find.byType(TextField)).controller!.text,
          'release-2',
          reason: 'the answer must stay in the field',
        );
        expect(link.calls('crewmate_action_answer'), isEmpty);
        await _unmount(t);
      },
    );

    testWidgets('a choice offline says so on the card, sends nothing', (
      t,
    ) async {
      final link = await _pump(
        t,
        _card({
          'kind': 'choose',
          'question': 'Which one?',
          'options': ['alpha', 'beta'],
        }),
      );
      await link.goOffline(t);

      await t.tap(find.text('beta'));
      await settle(t);

      expect(find.text(_offline), findsOneWidget);
      expect(link.calls('crewmate_action_answer'), isEmpty);
      await _unmount(t);
    });

    testWidgets('Cancel offline says so and cancels nothing', (t) async {
      final link = await _pump(t, _card(input));
      await link.goOffline(t);

      await t.tap(find.text('Cancel'));
      await settle(t);

      expect(find.text(_offline), findsOneWidget);
      expect(link.calls('crewmate_cancel'), isEmpty);
      await _unmount(t);
    });

    testWidgets('Connect GitHub offline says so and starts no login', (
      t,
    ) async {
      final link = await _pump(t, _card({'kind': 'connect_github'}));
      await link.goOffline(t);

      await t.tap(find.text('Connect'));
      await settle(t);

      expect(find.text(_offline), findsOneWidget);
      expect(link.calls('crew_github_login_start'), isEmpty);
      await _unmount(t);
    });

    testWidgets('a secret offline never asks for Face ID and never sends', (
      t,
    ) async {
      var asked = 0;
      final link = await _pump(
        t,
        _card({'kind': 'provide_secret', 'name': 'API_KEY'}),
        overrides: [
          deviceAuthProvider.overrideWithValue((_) async {
            asked++;
            return true;
          }),
        ],
      );
      await link.goOffline(t);

      await t.enterText(find.byType(TextField), 'ghp_secret');
      await t.tap(find.text('Submit'));
      await settle(t);

      expect(find.text(_offline), findsOneWidget);
      expect(link.calls('crewmate_action_answer'), isEmpty);
      expect(asked, 0, reason: 'no prompt for something that cannot go out');
      await _unmount(t);
    });

    testWidgets('online, Submit sends exactly one answer and shows no error', (
      t,
    ) async {
      final link = await _pump(t, _card(input));

      await t.enterText(find.byType(TextField), 'release-2');
      await t.tap(find.text('Submit'));
      await settle(t);

      final answers = link.calls('crewmate_action_answer');
      expect(answers, hasLength(1));
      expect(answers.single['args'], containsPair('value', 'release-2'));
      expect(find.textContaining('offline'), findsNothing);
      await _unmount(t);
    });
  });

  group('crewmate chat', () {
    Widget chat({String id = 'crewmate-1'}) =>
        CrewmateChatScreen(device: _mac, crewmateId: id);

    testWidgets('Allow offline says so; the approval stays, nothing is sent', (
      t,
    ) async {
      // The demo's Nightly crewmate is parked on a tool approval.
      final link = await _pump(t, chat(id: 'crewmate-2'));
      expect(find.text('Allow'), findsOneWidget);
      await link.goOffline(t);

      await t.tap(find.text('Allow'));
      await settle(t);

      expect(find.text(_offline), findsOneWidget);
      expect(find.text('Allow'), findsOneWidget, reason: 'still pending');
      expect(link.calls('agent_approve'), isEmpty);
      await _unmount(t);
    });

    testWidgets('Send offline says so and keeps the draft', (t) async {
      final handle = t.ensureSemantics();
      final link = await _pump(t, chat());
      await link.goOffline(t);

      await t.enterText(find.byType(TextField), 'Review this PR');
      await settle(t); // the send button grows in once there is text
      await t.tap(find.bySemanticsLabel('Send'));
      await settle(t);

      expect(find.text(_offline), findsOneWidget);
      expect(
        t.widget<TextField>(find.byType(TextField)).controller!.text,
        'Review this PR',
        reason: 'the draft must stay',
      );
      expect(link.calls('crewmate_task'), isEmpty);
      handle.dispose();
      await _unmount(t);
    });

    testWidgets('online, Send sends exactly one task and clears the draft', (
      t,
    ) async {
      final handle = t.ensureSemantics();
      final link = await _pump(t, chat());

      await t.enterText(find.byType(TextField), 'Review this PR');
      await settle(t); // the send button grows in once there is text
      await t.tap(find.bySemanticsLabel('Send'));
      await settle(t);

      expect(link.calls('crewmate_task'), hasLength(1));
      expect(t.widget<TextField>(find.byType(TextField)).controller!.text, '');
      expect(find.textContaining('offline'), findsNothing);
      handle.dispose();
      await _unmount(t);
    });
  });

  group('agent chat', () {
    Widget chat(String pane, String agent, String state) => AgentChatScreen(
      device: _mac,
      paneKey: pane,
      initial: AgentStatus(
        state: state,
        agent: agent,
        updatedAt: 0,
        stateStartedAt: DateTime.now().millisecondsSinceEpoch,
      ),
      title: 'agent',
    );
    Widget omp() => chat('demo-omp', 'omp', 'done');

    testWidgets('Send offline says so and keeps the draft', (t) async {
      final link = await _pump(t, omp());
      await link.goOffline(t);

      await t.enterText(find.byType(TextField), 'use z-60');
      await t.tap(find.byTooltip('Send message'));
      await settle(t);

      expect(find.text(_offline), findsOneWidget);
      expect(
        t.widget<TextField>(find.byType(TextField)).controller!.text,
        'use z-60',
        reason: 'the draft must stay',
      );
      expect(link.calls('agent_send'), isEmpty);
      await _unmount(t);
    });

    testWidgets('Stop offline says so and interrupts nothing', (t) async {
      // Opened from a list that said the agent is working; then the computer goes off.
      final link = await _pump(t, chat('demo-omp', 'omp', 'working'));
      await link.goOffline(t);
      expect(find.byTooltip('Stop'), findsWidgets);

      await t.tap(find.byTooltip('Stop').first);
      await settle(t);

      expect(find.text(_offline), findsOneWidget);
      expect(link.calls('agent_interrupt'), isEmpty);
      await _unmount(t);
    });

    testWidgets('online, Send sends exactly one message and clears the draft', (
      t,
    ) async {
      final link = await _pump(t, omp());

      await t.enterText(find.byType(TextField), 'use z-60');
      await t.tap(find.byTooltip('Send message'));
      await settle(t);

      expect(link.calls('agent_send'), hasLength(1));
      expect(t.widget<TextField>(find.byType(TextField)).controller!.text, '');
      expect(find.textContaining('offline'), findsNothing);
      await _unmount(t);
    });
  });
}
