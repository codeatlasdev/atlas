// The home project list and the agent launcher on a bad network day. Real providers, real
// connection, real screens: the computer is a scripted host (`_Host`) whose `project_list` /
// `agent_integrations` replies the test chooses (ok, error, held back).
//
// Before: a null connection or `ok:false` became `[]`, so "No projects yet" (or "No agent found")
// replaced a list that was merely unreachable, and the 5 s poll kept asking while the phone slept.
import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_launcher.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';

class _Storage extends MacStorage {
  _Storage(this._devices) : super(storage: null);
  final List<MacDevice> _devices;
  @override
  Future<List<MacDevice>> loadDevices() async => _devices;
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// A hostd the test scripts. `info` is answered at once; `project_list` and `agent_integrations`
/// answer with whatever [listReply] / [agentsReply] hold now (`null` = park the call until
/// [release]). Every other command is answered with an empty `ok`.
class _Host implements Transport {
  final _in = StreamController<String>.broadcast();
  final calls = <String>[];
  final _parked = <(String cmd, Object id)>[];

  /// False: the computer cannot be reached, so the next connect fails (as a Mac that went to sleep).
  bool online = true;

  /// The link dies under a live connection: the multiplexer ends and the notifier reconnects.
  Future<void> drop() => _in.close();

  Map<String, Object?>? listReply = {
    'ok': true,
    'data': {
      'projects': [_shop],
    },
  };
  Map<String, Object?>? agentsReply = {
    'ok': true,
    'data': {'agents': <Object>[]},
  };

  int count(String cmd) => calls.where((c) => c == cmd).length;

  @override
  Stream<String> get incomingLines => _in.stream;

  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    final cmd = req['cmd'] as String;
    final id = req['id']! as Object;
    calls.add(cmd);
    final reply = switch (cmd) {
      'info' => {
        'ok': true,
        'data': {'proto': kAppProto},
      },
      'project_list' => listReply,
      'agent_integrations' => agentsReply,
      _ => {'ok': true, 'data': <String, Object?>{}},
    };
    if (reply == null) {
      _parked.add((cmd, id));
      return;
    }
    _send(id, reply);
  }

  void _send(Object id, Map<String, Object?> reply) => scheduleMicrotask(() {
    if (!_in.isClosed) _in.add(jsonEncode({'id': id, ...reply}));
  });

  /// Answers every parked call to [cmd] with [reply].
  void release(String cmd, Map<String, Object?> reply) {
    for (final p in _parked.where((p) => p.$1 == cmd).toList()) {
      _parked.remove(p);
      _send(p.$2, reply);
    }
  }

  @override
  Future<void> close() => _in.close();
}

const _mac = MacDevice(nodeId: 'n1', name: 'My Mac');

const _shop = {
  'id': 'a',
  'root': '/Users/me/shop',
  'name': 'shop',
  'running': false,
  'services': <Object>[],
  'agents': <Object>[],
};

Future<void> _settle(WidgetTester t, [int ms = 600]) async {
  for (var i = 0; i < ms ~/ 50; i++) {
    await t.pump(const Duration(milliseconds: 50));
  }
}

/// The legal path a phone takes (resumed -> inactive -> hidden).
void _background(WidgetTester t) {
  t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
  t.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
}

void _foreground(WidgetTester t) {
  t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
  t.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
}

Future<ProviderContainer> _pump(
  WidgetTester t,
  _Host host,
  Widget home, {
  bool unreachable = false,
}) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  addTearDown(() => t.binding.setSurfaceSize(null));
  late ProviderContainer container;
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage([_mac])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => unreachable || !host.online
              ? throw StateError('no route to My Mac')
              : host,
        ),
      ],
      child: Builder(
        builder: (context) {
          container = ProviderScope.containerOf(context);
          return MaterialApp(theme: atlasTheme(), home: home);
        },
      ),
    ),
  );
  await _settle(t, 3000);
  return container;
}

Future<void> _unmount(WidgetTester t) async {
  await t.pumpWidget(const SizedBox());
  await _settle(t, 200);
}

void main() {
  group('home project list', () {
    testWidgets(
      'a computer that drops out keeps the last project list on screen, with the offline strip',
      (t) async {
        final host = _Host();
        await _pump(t, host, const HomeScreen());
        expect(find.text('shop'), findsOneWidget);

        // The link dies and the computer stays out of reach; several 5 s polls pass.
        host.online = false;
        await host.drop();
        await _settle(t, 16000);

        expect(
          find.text('shop'),
          findsOneWidget,
          reason:
              'a list that was loaded stays until the computer answers again',
        );
        expect(find.text('No projects yet'), findsNothing);
        expect(
          find.textContaining('My Mac is offline'),
          findsOneWidget,
          reason: 'the list is shown with the offline strip, not behind a full-screen state',
        );
        await _unmount(t);
      },
    );

    testWidgets(
      'a project_list error shows the error with Try again, not "No projects yet"',
      (t) async {
        final host = _Host()..listReply = {'ok': false, 'error': 'forbidden'};
        await _pump(t, host, const HomeScreen());

        expect(find.text('No projects yet'), findsNothing);
        expect(find.text('Try again'), findsOneWidget);
        // Said for a person (the raw "forbidden" is under Details): a permission to grant on the computer.
        expect(
          find.text("This phone isn't allowed to do that"),
          findsOneWidget,
        );

        // Try again asks again, and a good answer replaces the error.
        host.listReply = {
          'ok': true,
          'data': {
            'projects': [_shop],
          },
        };
        final before = host.count('project_list');
        await t.tap(find.text('Try again'));
        await _settle(t);
        expect(host.count('project_list'), greaterThan(before));
        expect(find.text('shop'), findsOneWidget);
        await _unmount(t);
      },
    );

    testWidgets('"No projects yet" shows only for a successful empty answer', (
      t,
    ) async {
      final host = _Host()
        ..listReply = {
          'ok': true,
          'data': {'projects': <Object>[]},
        };
      await _pump(t, host, const HomeScreen());
      expect(find.text('No projects yet'), findsOneWidget);
      expect(find.text('Try again'), findsNothing);
      await _unmount(t);
    });

    testWidgets(
      'no project_list is sent while the app is in the background, and polling resumes after',
      (t) async {
        final host = _Host();
        await _pump(t, host, const HomeScreen());
        final loaded = host.count('project_list');
        expect(loaded, greaterThan(0));

        _background(t);
        await _settle(
          t,
          16000,
        ); // three poll ticks, inside the 30 s connection grace
        expect(
          host.count('project_list'),
          loaded,
          reason: 'the poll must rest while the app is backgrounded',
        );

        _foreground(t);
        await _settle(t, 6000);
        expect(host.count('project_list'), greaterThan(loaded));
        await _unmount(t);
      },
    );

    testWidgets('no project_list is sent while the computer is offline', (
      t,
    ) async {
      final host = _Host();
      await _pump(t, host, const HomeScreen());

      // The link dies and the computer stays out of reach (the notifier retries with backoff and
      // fails every time).
      host.online = false;
      await host.drop();
      await _settle(t, 3000);
      final whenDown = host.count('project_list');

      await _settle(t, 16000);
      expect(
        host.count('project_list'),
        whenDown,
        reason: 'the poll must not ask a computer that is not connected',
      );
      await _unmount(t);
    });

    testWidgets(
      'polls ticking while one is in flight put ONE project_list on the wire',
      (t) async {
        final host = _Host();
        await _pump(t, host, const HomeScreen());
        host.listReply = null; // the next list never answers
        await _settle(t, 6000); // first tick: parked
        final inFlight = host.count('project_list');
        await _settle(t, 16000); // three more ticks
        expect(
          host.count('project_list'),
          inFlight,
          reason: 'a poll must not queue behind an unanswered request',
        );
        host.release('project_list', {
          'ok': true,
          'data': {
            'projects': [_shop],
          },
        });
        await _unmount(t);
      },
    );

    // An unreachable computer says so at once. A provider that throws is retried by Riverpod (10
    // times, 0.2 s to 6.4 s) and reads as "loading" meanwhile, which kept this screen on a spinner
    // for about 45 s before it said why.
    testWidgets(
      'an unreachable computer shows "Can\'t reach" within seconds, with no spinner',
      (t) async {
        final host = _Host();
        await _pump(t, host, const HomeScreen(), unreachable: true);
        expect(find.text("Can't reach My Mac"), findsOneWidget);
        expect(find.byType(CircularProgressIndicator), findsNothing);
        expect(find.text('No projects yet'), findsNothing);
        expect(host.count('project_list'), 0);
        await _unmount(t);
      },
    );
  });

  group('agent launcher', () {
    Future<_Host> openLauncher(WidgetTester t, _Host host) async {
      await _pump(
        t,
        host,
        Scaffold(
          body: Consumer(
            builder: (context, ref, _) => TextButton(
              onPressed: () =>
                  showAgentLauncher(context, device: _mac, projectId: 'a'),
              child: const Text('open launcher'),
            ),
          ),
        ),
      );
      await t.tap(find.text('open launcher'));
      await _settle(t);
      return host;
    }

    testWidgets(
      'an agent_integrations error is an error, not "No agent found"',
      (t) async {
        final host = _Host()..agentsReply = {'ok': false, 'error': 'forbidden'};
        await openLauncher(t, host);
        expect(find.textContaining('No agent found'), findsNothing);
        expect(find.text('Try again'), findsOneWidget);
        await _unmount(t);
      },
    );

    testWidgets('"No agent found" shows for a successful empty answer', (
      t,
    ) async {
      await openLauncher(t, _Host());
      expect(find.textContaining('No agent found'), findsOneWidget);
      expect(find.text('Try again'), findsNothing);
      await _unmount(t);
    });

    // The sheet reads the provider itself (no home screen above it to say "Can't reach"). A provider
    // that throws is retried by Riverpod and reads as loading for about 45 s; an offline computer
    // must be said at once.
    testWidgets(
      'an unreachable computer shows its error within seconds, not a spinner',
      (t) async {
        await _pump(
          t,
          _Host(),
          Scaffold(
            body: Consumer(
              builder: (context, ref, _) => TextButton(
                onPressed: () =>
                    showAgentLauncher(context, device: _mac, projectId: 'a'),
                child: const Text('open launcher'),
              ),
            ),
          ),
          unreachable: true,
        );
        await t.tap(find.text('open launcher'));
        await _settle(t, 1500);
        expect(find.text('My Mac is offline'), findsOneWidget);
        expect(find.text('Try again'), findsOneWidget);
        expect(find.byType(CircularProgressIndicator), findsNothing);
        expect(find.textContaining('No agent found'), findsNothing);
        await _unmount(t);
      },
    );
  });
}
