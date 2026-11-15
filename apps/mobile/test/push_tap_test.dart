// Tapping a push opens that agent's chat. The push says only which pane (and a title); the phone
// finds the computer that has it, so these run against computers that answer `agent_status_all` the
// way hostd does, with a real widget tree for the screen that opens.
import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_payload.dart';
import 'package:atlas_mobile/core/push/push_tap.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';

class _Storage extends MacStorage {
  _Storage(this.devices) : super(storage: null);
  final List<MacDevice> devices;
  @override
  Future<List<MacDevice>> loadDevices() async => devices;
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// A computer with some agent panes; answers like hostd's `agent_status_all`.
class _Computer implements Transport {
  _Computer(this.panes, {this.fail = false});
  final Map<String, String> panes; // pane key -> agent state
  final bool fail;
  final _lines = StreamController<String>.broadcast();

  @override
  Stream<String> get incomingLines => _lines.stream;

  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    if (fail && req['cmd'] == 'agent_status_all') {
      _lines.add(jsonEncode({'id': req['id'], 'ok': false, 'error': 'boom'}));
      return;
    }
    final Object data = switch (req['cmd']) {
      'agent_status_all' => {
        'panes': {
          for (final e in panes.entries)
            e.key: {
              'state': e.value,
              'agent': 'omp',
              'updated_at': 1,
              'state_started_at': 1,
            },
        },
        'hook_port': 1,
        'pending_approvals': [],
      },
      'info' => {
        'proto': kAppProto,
        'caps': ['push'],
      },
      _ => {},
    };
    _lines.add(jsonEncode({'id': req['id'], 'ok': true, 'data': data}));
  }

  @override
  Future<void> close() async {}
}

const _mac1 = MacDevice(nodeId: 'mac-1', name: 'Studio');
const _mac2 = MacDevice(nodeId: 'mac-2', name: 'Laptop');

ProviderContainer _container(Map<String, _Computer> byNode) {
  final c = ProviderContainer(
    overrides: [
      macStorageProvider.overrideWithValue(_Storage([_mac1, _mac2])),
      transportFactoryProvider.overrideWithValue(
        (d, _) async => byNode[d.nodeId]!,
      ),
    ],
  );
  addTearDown(c.dispose);
  return c;
}

PushPayload _agent(String pane, {PushKind kind = PushKind.agentDone}) =>
    PushPayload(kind: kind, pane: pane, title: 'omp · atlas');

void main() {
  // Two Macs paired: the pane lives on exactly one. Asking the wrong one first must not open a
  // chat on it.
  test(
    'finds the computer that has the pane, whichever is asked first',
    () async {
      final c = _container({
        'mac-1': _Computer({'other': 'idle'}),
        'mac-2': _Computer({'pane-7': 'done'}),
      });
      final t =
          (await resolvePushTarget(c, _agent('pane-7')) as PushFound).target;
      expect(t.device.nodeId, 'mac-2');
      expect(t.paneKey, 'pane-7');
      expect(t.status.state, 'done');
    },
  );

  test('the agent is gone (closed on the Mac): no target', () async {
    final c = _container({
      'mac-1': _Computer({}),
      'mac-2': _Computer({'other': 'idle'}),
    });
    expect(await resolvePushTarget(c, _agent('pane-7')), isA<PushGone>());
  });

  // A computer that is off must not stop the search on the one that is on.
  test(
    'an unreachable computer does not hide the one that has the pane',
    () async {
      final c = ProviderContainer(
        overrides: [
          macStorageProvider.overrideWithValue(_Storage([_mac1, _mac2])),
          transportFactoryProvider.overrideWithValue((d, _) async {
            if (d.nodeId == 'mac-1') throw StateError('offline');
            return _Computer({'pane-7': 'blocked'});
          }),
        ],
      );
      addTearDown(c.dispose);
      final t =
          (await resolvePushTarget(c, _agent('pane-7')) as PushFound).target;
      expect(t.device.nodeId, 'mac-2');
      expect(t.status.state, 'blocked');
    },
  );

  // The pane is not on the computers that answered, but one did not: the agent may be on it, so
  // "finished" would be a lie. The user is told which computer could not be asked.
  test(
    'no computer has the pane and one is unreachable: unreachable, by name',
    () async {
      final c = ProviderContainer(
        overrides: [
          macStorageProvider.overrideWithValue(_Storage([_mac1, _mac2])),
          transportFactoryProvider.overrideWithValue((d, _) async {
            if (d.nodeId == 'mac-1') throw StateError('offline');
            return _Computer({'other': 'idle'});
          }),
        ],
      );
      addTearDown(c.dispose);
      final r = await resolvePushTarget(c, _agent('pane-7'));
      expect(r, isA<PushUnreachable>());
      expect((r as PushUnreachable).names, ['Studio']);
    },
  );

  test(
    'a computer that answers an error to the question is unreachable',
    () async {
      final c = ProviderContainer(
        overrides: [
          macStorageProvider.overrideWithValue(_Storage([_mac1])),
          transportFactoryProvider.overrideWithValue(
            (d, _) async => _Computer({}, fail: true),
          ),
        ],
      );
      addTearDown(c.dispose);
      expect(
        await resolvePushTarget(c, _agent('pane-7')),
        isA<PushUnreachable>(),
      );
    },
  );

  // Crew pushes carry no pane: nothing to resolve here (they open the Crew tab).
  test('a crew push has no agent to open', () async {
    final c = _container({
      'mac-1': _Computer({'pane-7': 'done'}),
      'mac-2': _Computer({}),
    });
    const crew = PushPayload(kind: PushKind.taskDone);
    expect(await resolvePushTarget(c, crew), isA<PushNoAgent>());
  });

  // The whole path: a tap lands in the provider the app root watches, and the app opens that agent's
  // chat on the right computer, titled from the push.
  testWidgets('a tap opens the agent chat on the computer that has the pane', (
    tester,
  ) async {
    final container = _container({
      'mac-1': _Computer({}),
      'mac-2': _Computer({'pane-7': 'done'}),
    });
    final navKey = GlobalKey<NavigatorState>();
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: MaterialApp(
          theme: atlasTheme(),
          navigatorKey: navKey,
          home: PushTapHost(
            navigatorKey: navKey,
            child: const Scaffold(body: Text('home')),
          ),
        ),
      ),
    );
    expect(find.byType(AgentChatScreen), findsNothing);

    container.read(pushTapProvider.notifier).state = _agent('pane-7');
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 100));

    final screen = tester.widget<AgentChatScreen>(find.byType(AgentChatScreen));
    expect(screen.device.nodeId, 'mac-2');
    expect(screen.paneKey, 'pane-7');
    expect(screen.title, 'omp · atlas');
    // Consumed: a rebuild does not open it a second time.
    expect(container.read(pushTapProvider), isNull);

    // Leave the way the app does (the chat's timers die with it).
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(seconds: 30));
  });

  Future<ProviderContainer> hostWith(
    WidgetTester tester,
    ProviderContainer container,
  ) async {
    final navKey = GlobalKey<NavigatorState>();
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: MaterialApp(
          theme: atlasTheme(),
          navigatorKey: navKey,
          home: PushTapHost(
            navigatorKey: navKey,
            child: const Scaffold(body: Text('home')),
          ),
        ),
      ),
    );
    return container;
  }

  Future<void> tap(
    WidgetTester tester,
    ProviderContainer c,
    PushPayload p,
  ) async {
    c.read(pushTapProvider.notifier).state = p;
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump(const Duration(milliseconds: 100));
  }

  // Regression: a tapped push whose agent had closed opened nothing, with no word why.
  testWidgets('a push whose agent is gone says the agent has finished', (
    tester,
  ) async {
    final c = _container({'mac-1': _Computer({}), 'mac-2': _Computer({})});
    await hostWith(tester, c);
    await tap(tester, c, _agent('pane-7'));
    expect(find.byType(AgentChatScreen), findsNothing);
    expect(find.text('That agent has finished'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('a push when the computer is unreachable names it', (
    tester,
  ) async {
    final c = ProviderContainer(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage([_mac1])),
        transportFactoryProvider.overrideWithValue(
          (d, _) async => throw StateError('offline'),
        ),
      ],
    );
    addTearDown(c.dispose);
    await hostWith(tester, c);
    await tap(tester, c, _agent('pane-7'));
    expect(find.byType(AgentChatScreen), findsNothing);
    expect(find.text('Studio is not reachable'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('a crew push opens no agent and says nothing', (tester) async {
    final c = _container({'mac-1': _Computer({}), 'mac-2': _Computer({})});
    await hostWith(tester, c);
    await tap(tester, c, const PushPayload(kind: PushKind.taskDone));
    expect(find.byType(SnackBar), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });
}
