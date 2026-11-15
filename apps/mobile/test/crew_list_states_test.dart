// Regression: the Crew tab showed "No crew yet" (and a Recruit-less empty screen) for as long as the
// computers' crew lists were still loading, because it read `value ?? []`. A slow link looked like a
// crew that did not exist. While anything is loading the tab shows a loader; "No crew yet" is for a
// settled, empty answer.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/computer_state.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';

import 'support/demo_harness.dart';

const _mac = MacDevice(nodeId: 'mac-1', name: 'Mac');

CrewListResult _empty() =>
    (isCrewServer: true, hostName: 'Mac', crewmates: const [], paused: false);

/// hostd's own answer to `crewmate_list` on a computer that is no crew server (crates/atlas-host
/// `crew::handle`, the `!hub.crew_host` branch). The one error that means "not a crew server".
const _notCrewServer =
    'this computer is not a crew server (atlas crew install)';

/// A computer for the REAL providers: answers `info` like a current hostd, `crew_servers` with none, and every
/// other command with [reply] (`null`: never, so a call stays pending). The test sees what was asked in [asked].
class _Computer implements Transport {
  _Computer(this.reply);
  Map<String, Object?>? Function(String cmd) reply;
  final asked = <String>[];
  final _in = StreamController<String>.broadcast();
  bool closed = false;

  @override
  Stream<String> get incomingLines => _in.stream;

  @override
  Future<void> close() async {
    closed = true;
    await _in.close();
  }

  /// The link dies (the Mac sleeps, the network drops).
  Future<void> drop() => close();

  @override
  Future<void> sendLine(String line) async {
    final m = jsonDecode(line) as Map<String, dynamic>;
    final cmd = m['cmd'] as String;
    asked.add(cmd);
    final body = switch (cmd) {
      'info' => {
        'ok': true,
        'data': {'proto': kAppProto},
      },
      'crew_servers' => {
        'ok': true,
        'data': {'servers': []},
      },
      _ => reply(cmd),
    };
    if (body == null) return;
    scheduleMicrotask(() {
      if (!_in.isClosed) _in.add(jsonEncode({'id': m['id'], ...body}));
    });
  }
}

Map<String, Object?> _ok(Object data) => {'ok': true, 'data': data};
Map<String, Object?> _err(String e) => {'ok': false, 'error': e};

/// A `crewmate_list` reply with one crewmate, in the shape hostd really sends it (contract fixture
/// `test/fixtures/crewmate_list.json`, made by hostd's own test).
Map<String, Object?> _oneCrewmate() {
  final real = jsonDecode(
    File('test/fixtures/crewmate_list.json').readAsStringSync(),
  ) as Map<String, dynamic>;
  final first = (real['crewmates'] as List).first as Map<String, dynamic>;
  return _ok({
    ...real,
    'crewmates': [first],
  });
}

/// The tab against real providers whose one computer is [computer].
Future<ProviderContainer> _pump(
  WidgetTester t,
  Widget screen,
  _Computer Function() computer,
) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  late ProviderContainer container;
  await t.pumpWidget(
    demoWrap(
      Builder(
        builder: (context) {
          container = ProviderScope.containerOf(context);
          return screen;
        },
      ),
      [_mac],
      transport: computer,
    ),
  );
  return container;
}

const _macB = MacDevice(nodeId: 'mac-2', name: 'Studio');

/// The tab against real providers whose paired computers are [by] (each answers on its own transport).
Future<void> _pumpMany(WidgetTester t, Map<MacDevice, _Computer> by) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage(by.keys.toList())),
        transportFactoryProvider.overrideWithValue(
          (d, _) async =>
              by.entries.firstWhere((e) => e.key.nodeId == d.nodeId).value,
        ),
      ],
      child: MaterialApp(theme: atlasTheme(), home: const CrewScreen()),
    ),
  );
}

/// Pumps long enough for microtasks and the connect + request round trips, not for the 30 s backoff.
Future<void> _settleShort(WidgetTester t) async {
  for (var i = 0; i < 10; i++) {
    await t.pump(const Duration(milliseconds: 50));
  }
}

class _Pending extends CrewNotifier {
  _Pending(super.device, this.gate);
  final Completer<CrewListResult> gate;
  @override
  Future<CrewListResult> build() => gate.future;
}

void main() {
  testWidgets(
    'crew tab shows a loader, not "No crew yet", while the list loads',
    (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      final gate = Completer<CrewListResult>();
      await t.pumpWidget(
        demoWrap(
          const CrewScreen(),
          [_mac],
          overrides: [
            crewDevicesProvider.overrideWith((_) async => [_mac]),
            crewProvider.overrideWith2((_) => _Pending(_mac, gate)),
          ],
        ),
      );
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));

      expect(find.text('No crew yet'), findsNothing);
      expect(find.byType(CircularProgressIndicator), findsOneWidget);

      // The answer arrives and it really is empty: now the empty state is true.
      gate.complete(_empty());
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));
      expect(find.byType(CircularProgressIndicator), findsNothing);
      expect(find.text('No crew yet'), findsOneWidget);

      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'crew tab shows a loader while the computers themselves are listed',
    (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      final devices = Completer<List<MacDevice>>();
      await t.pumpWidget(
        demoWrap(
          const CrewScreen(),
          [_mac],
          overrides: [crewDevicesProvider.overrideWith((_) => devices.future)],
        ),
      );
      await t.pump();

      expect(find.text('No crew yet'), findsNothing);
      expect(find.byType(CircularProgressIndicator), findsOneWidget);

      devices.complete(const []);
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));
      expect(find.text('No crew yet'), findsOneWidget);

      await t.pumpWidget(const SizedBox());
    },
  );

  // Same class as the tab: the thread read `value ?? []`, so "Say hi" showed while the tasks loaded.
  testWidgets(
    'crewmate chat shows a loader, not "Say hi", while its tasks load',
    (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      final gate = Completer<List<CrewmateTask>>();
      await t.pumpWidget(
        demoWrap(
          const CrewmateChatScreen(
            device: _mac,
            crewmateId: 'crewmate-working',
          ),
          [_mac],
          overrides: [
            crewmateTasksProvider.overrideWith((_, _) => gate.future),
          ],
        ),
      );
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));

      expect(find.textContaining('Say hi'), findsNothing);
      expect(find.byType(CircularProgressIndicator), findsOneWidget);

      gate.complete(const []);
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));
      expect(find.byType(CircularProgressIndicator), findsNothing);

      await t.pumpWidget(const SizedBox());
    },
  );

  // One strip, one sentence: the chat says its computer is offline with the shared message.
  testWidgets('offline strip shows the one sentence, only while offline', (
    t,
  ) async {
    final c = ProviderContainer(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage([_mac])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => throw Exception('refused'),
        ),
      ],
    );
    await t.pumpWidget(
      UncontrolledProviderScope(
        container: c,
        child: MaterialApp(
          theme: atlasTheme(),
          home: Scaffold(
            body: OfflineStrip(device: _mac, suffix: ' · history shown'),
          ),
        ),
      ),
    );
    await t.pump();
    await t.pump(const Duration(milliseconds: 100));
    expect(find.text('Mac is offline · history shown'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    // The reconnect backoff timer lives with the container: dispose it inside the test.
    c.dispose();
  });

  group('real providers', () {
    // F1: the connection is still opening. The provider used to answer "not a crew server" at once.
    testWidgets('crew tab: connection still opening is not "No crew yet"', (
      t,
    ) async {
      final dial = Completer<Transport>();
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        ProviderScope(
          overrides: [
            macStorageProvider.overrideWithValue(DemoStorage(const [_mac])),
            transportFactoryProvider.overrideWithValue((_, _) => dial.future),
          ],
          child: MaterialApp(theme: atlasTheme(), home: const CrewScreen()),
        ),
      );
      await _settleShort(t);
      expect(find.text('No crew yet'), findsNothing);
      expect(find.byType(CircularProgressIndicator), findsOneWidget);

      // It connects and the crew really is empty-of-a-server: only now the empty state is true.
      final computer = _Computer((_) => _err(_notCrewServer));
      dial.complete(computer);
      await _settleShort(t);
      expect(find.byType(CircularProgressIndicator), findsNothing);
      expect(find.text('No crew yet'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
      await computer.close();
    });

    // F1: hostd's real "not a crew server" answer IS a settled empty result (a Mac with no crew).
    testWidgets('crew tab: hostd "not a crew server" is the real empty state', (
      t,
    ) async {
      final computer = _Computer((_) => _err(_notCrewServer));
      await _pump(t, const CrewScreen(), () => computer);
      await _settleShort(t);
      expect(find.text('No crew yet'), findsOneWidget);
      expect(find.text("Couldn't read the crew"), findsNothing);
      await t.pumpWidget(const SizedBox());
      await computer.close();
    });

    // F1: any other failure (forbidden, timeout) used to look like "not a crew server".
    testWidgets('crew tab: "forbidden" is an error with Try again', (t) async {
      var listCalls = 0;
      final computer = _Computer((cmd) {
        if (cmd != 'crewmate_list') return _ok({});
        listCalls++;
        return listCalls == 1 ? _err('forbidden') : _oneCrewmate();
      });
      await _pump(t, const CrewScreen(), () => computer);
      await _settleShort(t);
      expect(find.text('No crew yet'), findsNothing);
      expect(find.text("Couldn't read the crew"), findsOneWidget);
      expect(find.text('forbidden'), findsOneWidget);

      await t.tap(find.text('Try again'));
      await _settleShort(t);
      expect(find.text("Couldn't read the crew"), findsNothing);
      expect(find.text('Reviewer'), findsWidgets);
      await t.pumpWidget(const SizedBox());
      await computer.close();
    });

    // F1: the link drops after the list loaded; a reconnect drops the connection's value and the list used to
    // collapse to "No crew yet". It keeps what it has and says the computer is off.
    testWidgets('crew tab: a dropped link keeps the loaded crew', (t) async {
      var dials = 0;
      late _Computer first;
      final computer = _Computer((cmd) => _oneCrewmate());
      first = computer;
      final second = _Computer((_) => null); // the redial never answers
      await _pump(t, const CrewScreen(), () {
        dials++;
        return dials == 1 ? first : second;
      });
      await _settleShort(t);
      expect(find.text('Reviewer'), findsWidgets);

      await first.drop();
      // The backoff timer (1 s) fires, the connection reloads: its value is gone for a while.
      for (var i = 0; i < 30; i++) {
        await t.pump(const Duration(milliseconds: 100));
      }
      expect(find.text('No crew yet'), findsNothing);
      expect(find.text('Reviewer'), findsWidgets);
      await t.pumpWidget(const SizedBox());
      await second.close();
    });

    // F1: a computer that refuses the connection is "Can't reach", at once. Riverpod's own retry (10 times,
    // up to 6.4 s apart) reported it as loading for about 45 s, so the tab sat on a spinner first.
    testWidgets('crew tab: an unreachable computer says so, with Try again', (
      t,
    ) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        ProviderScope(
          overrides: [
            macStorageProvider.overrideWithValue(DemoStorage(const [_mac])),
            transportFactoryProvider.overrideWithValue(
              (_, _) async => throw Exception('refused'),
            ),
          ],
          child: MaterialApp(theme: atlasTheme(), home: const CrewScreen()),
        ),
      );
      // Well under the first retry delay of the old behaviour pile-up: the state must already be the final one.
      await _settleShort(t);
      expect(find.text('No crew yet'), findsNothing);
      expect(find.text("Can't reach Mac"), findsOneWidget);
      expect(find.text('Try again'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
    });

    testWidgets('crewmate chat: an unreachable computer says so', (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        ProviderScope(
          overrides: [
            macStorageProvider.overrideWithValue(DemoStorage(const [_mac])),
            transportFactoryProvider.overrideWithValue(
              (_, _) async => throw Exception('refused'),
            ),
          ],
          child: MaterialApp(
            theme: atlasTheme(),
            home: const CrewmateChatScreen(
              device: _mac,
              crewmateId: 'crewmate-working',
            ),
          ),
        ),
      );
      await _settleShort(t);
      expect(find.textContaining('Say hi'), findsNothing);
      expect(find.text("Can't reach Mac"), findsOneWidget);
      await t.pumpWidget(const SizedBox());
    });

    // F1: the thread said "Say hi to X" for a thread that failed to load.
    testWidgets('crewmate chat: failed task list is an error, not "Say hi"', (
      t,
    ) async {
      final computer = _Computer((cmd) {
        if (cmd == 'crewmate_list') return _oneCrewmate();
        if (cmd == 'crewmate_tasks') return _err('forbidden');
        return _ok({});
      });
      await _pump(
        t,
        const CrewmateChatScreen(device: _mac, crewmateId: 'crewmate-working'),
        () => computer,
      );
      await _settleShort(t);
      expect(find.textContaining('Say hi'), findsNothing);
      expect(find.text('forbidden'), findsOneWidget);
      expect(find.text('Try again'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
      await computer.close();
    });

    testWidgets('crewmate chat: connection opening is not "Say hi"', (t) async {
      final dial = Completer<Transport>();
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        ProviderScope(
          overrides: [
            macStorageProvider.overrideWithValue(DemoStorage(const [_mac])),
            transportFactoryProvider.overrideWithValue((_, _) => dial.future),
          ],
          child: MaterialApp(
            theme: atlasTheme(),
            home: const CrewmateChatScreen(
              device: _mac,
              crewmateId: 'crewmate-working',
            ),
          ),
        ),
      );
      await _settleShort(t);
      expect(find.textContaining('Say hi'), findsNothing);
      expect(find.byType(CircularProgressIndicator), findsWidgets);
      await t.pumpWidget(const SizedBox());
      dial.complete(_Computer((_) => null));
    });

    // F2: the Inbox said "All clear" while the computers were still being listed, and after that listing failed.
    testWidgets('inbox: crew devices pending is not "All clear"', (t) async {
      final gate = Completer<List<MacDevice>>();
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        demoWrap(
          const InboxScreen(),
          [_mac],
          overrides: [crewDevicesProvider.overrideWith((_) => gate.future)],
        ),
      );
      await _settleShort(t);
      expect(find.text('All clear'), findsNothing);
      expect(find.byType(CircularProgressIndicator), findsOneWidget);

      gate.complete(const []);
      await _settleShort(t);
      expect(find.text('All clear'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
    });

    testWidgets('inbox: a failed crew devices listing is an error', (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        demoWrap(
          const InboxScreen(),
          [_mac],
          overrides: [
            crewDevicesProvider.overrideWith(
              (_) async => throw StateError('storage unreadable'),
            ),
          ],
        ),
      );
      await _settleShort(t);
      expect(find.text('All clear'), findsNothing);
      expect(find.text('storage unreadable'), findsOneWidget);
      expect(find.text('Try again'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
    });

    // F2: a crew server whose list is still loading must not let the inbox claim "All clear".
    testWidgets('inbox: a crew list still loading is not "All clear"', (
      t,
    ) async {
      final computer = _Computer(
        (cmd) => cmd == 'crewmate_list' ? null : _ok({'projects': []}),
      );
      await _pump(t, const InboxScreen(), () => computer);
      await _settleShort(t);
      expect(find.text('All clear'), findsNothing);
      await t.pumpWidget(const SizedBox());
      await computer.close();
    });
  });

  // Gap of the first fix: with several computers, when all failed and nothing had loaded the tab named only the
  // first computer's error, and "Try again" re-asked every computer. Each computer owns its state.
  group('several computers', () {
    int listCalls(_Computer c) =>
        c.asked.where((x) => x == 'crewmate_list').length;

    testWidgets(
      'two computers failing differently: each error shown, each named',
      (t) async {
        final a = _Computer(
          (cmd) => cmd == 'crewmate_list' ? _err('forbidden') : _ok({}),
        );
        final b = _Computer(
          (cmd) => cmd == 'crewmate_list' ? _err('queue is full') : _ok({}),
        );
        await _pumpMany(t, {_mac: a, _macB: b});
        await _settleShort(t);

        expect(find.text('No crew yet'), findsNothing);
        expect(find.text('forbidden'), findsOneWidget);
        expect(find.text('queue is full'), findsOneWidget);
        expect(find.textContaining('Mac'), findsWidgets);
        expect(find.textContaining('Studio'), findsWidgets);
        expect(find.text('Try again'), findsNWidgets(2));
        await t.pumpWidget(const SizedBox());
        await a.close();
        await b.close();
      },
    );

    testWidgets('Try again on one computer asks only that computer', (t) async {
      final a = _Computer(
        (cmd) => cmd == 'crewmate_list' ? _err('forbidden') : _ok({}),
      );
      final b = _Computer(
        (cmd) => cmd == 'crewmate_list' ? _err('queue is full') : _ok({}),
      );
      await _pumpMany(t, {_mac: a, _macB: b});
      await _settleShort(t);
      final (beforeA, beforeB) = (listCalls(a), listCalls(b));

      // The first card is Mac's (paired order).
      await t.tap(find.text('Try again').first);
      await _settleShort(t);

      expect(listCalls(a), beforeA + 1);
      expect(listCalls(b), beforeB);
      await t.pumpWidget(const SizedBox());
      await a.close();
      await b.close();
    });

    testWidgets('a computer that loaded keeps its crew while another fails', (
      t,
    ) async {
      final a = _Computer(
        (cmd) => cmd == 'crewmate_list' ? _oneCrewmate() : _ok({}),
      );
      final b = _Computer(
        (cmd) => cmd == 'crewmate_list' ? _err('forbidden') : _ok({}),
      );
      await _pumpMany(t, {_mac: a, _macB: b});
      await _settleShort(t);

      expect(find.text('Reviewer'), findsWidgets);
      expect(find.text('forbidden'), findsOneWidget);
      expect(find.textContaining('Studio'), findsWidgets);
      expect(find.text('No crew yet'), findsNothing);
      await t.pumpWidget(const SizedBox());
      await a.close();
      await b.close();
    });

    testWidgets('both answering with no crew is "No crew yet"', (t) async {
      final a = _Computer(
        (cmd) => cmd == 'crewmate_list' ? _err(_notCrewServer) : _ok({}),
      );
      final b = _Computer(
        (cmd) => cmd == 'crewmate_list' ? _err(_notCrewServer) : _ok({}),
      );
      await _pumpMany(t, {_mac: a, _macB: b});
      await _settleShort(t);

      expect(find.text('No crew yet'), findsOneWidget);
      expect(find.text('Try again'), findsNothing);
      await t.pumpWidget(const SizedBox());
      await a.close();
      await b.close();
    });

    testWidgets(
      'one computer still loading and one failed: its error is shown, the other keeps its spinner',
      (t) async {
        final a = _Computer((cmd) => cmd == 'crewmate_list' ? null : _ok({}));
        final b = _Computer(
          (cmd) => cmd == 'crewmate_list' ? _err('forbidden') : _ok({}),
        );
        await _pumpMany(t, {_mac: a, _macB: b});
        await _settleShort(t);

        expect(find.text('No crew yet'), findsNothing);
        expect(find.text('forbidden'), findsOneWidget);
        expect(find.byType(CircularProgressIndicator), findsOneWidget);
        await t.pumpWidget(const SizedBox());
        await a.close();
        await b.close();
      },
    );
  });
}
