import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/projects/charts.dart';
import 'package:atlas_mobile/features/projects/project_detail_screen.dart';

// A slow link answers out of order. The project panel and its logs page must show the reply to the
// NEWEST question, never roll back to a late one, and never stack requests behind a slow one.
// Real screens and a real connection; the computer is a FakeTransport-style host whose replies the
// test holds back and releases (gated completers), so the order of delivery is the test's.

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// A hostd whose `project_detail` and `project_logs` replies wait for the test. Every other command
/// (`info`, the changes tile, ...) is answered at once with an empty `ok`.
class _Host implements Transport {
  final _in = StreamController<String>.broadcast();
  final calls = <(String, Map<String, dynamic>)>[];

  /// Gated commands: each call is parked here, in order, until [reply] releases it.
  final _gated = <String, List<(Object id, Map<String, dynamic> args)>>{};

  @override
  Stream<String> get incomingLines => _in.stream;

  Iterable<Map<String, dynamic>> sent(String cmd) =>
      calls.where((c) => c.$1 == cmd).map((c) => c.$2);

  /// Calls to [cmd] still waiting for a reply.
  int pending(String cmd) => _gated[cmd]?.length ?? 0;

  @override
  Future<void> sendLine(String line) async {
    final req = jsonDecode(line) as Map<String, dynamic>;
    final cmd = req['cmd'] as String;
    final args = (req['args'] as Map?)?.cast<String, dynamic>() ?? const {};
    calls.add((cmd, args));
    if (cmd == 'project_detail' || cmd == 'project_logs') {
      (_gated[cmd] ??= []).add((req['id']! as Object, args));
      return;
    }
    _answer(req['id']! as Object, cmd == 'info' ? {'proto': kAppProto} : {});
  }

  void _answer(Object id, Object? data) => scheduleMicrotask(() {
    if (!_in.isClosed) {
      _in.add(jsonEncode({'id': id, 'ok': true, 'data': data}));
    }
  });

  /// Releases the [index]-th parked call to [cmd] (0 = the oldest still waiting) with [data].
  void reply(String cmd, int index, Object? data) {
    final (id, _) = _gated[cmd]!.removeAt(index);
    _answer(id, data);
  }

  @override
  Future<void> close() => _in.close();
}

const _mac = MacDevice(nodeId: 'n1', name: 'My Mac');

final _project = ProjectInfo.fromJson({
  'id': 'a',
  'root': '/Users/me/shop',
  'name': 'shop',
  'running': false,
  'services': <Object>[],
  'agents': <Object>[],
});

/// The producer's real `project_detail` (fixture written by atlas-host), with the live status of the
/// `api` service overridden: the memory figure tells one reply from another.
Map<String, dynamic> _detail(double memMb) {
  final base = jsonDecode(
    File('test/fixtures/project_detail.json').readAsStringSync(),
  ) as Map<String, dynamic>;
  return {
    ...base,
    'status': {
      'running': true,
      'pid': 42,
      'services': [
        {
          'name': 'api',
          'state': 'running',
          'port': 3000,
          'mem_mb': memMb,
          'cpu': 1.0,
          'restarts': 0,
          'uptime_s': 60,
        },
        {
          'name': 'db',
          'state': 'running',
          'mem_mb': 1.0,
          'cpu': 0.0,
          'restarts': 0,
          'uptime_s': 60,
        },
      ],
    },
  };
}

Future<void> _pump(WidgetTester t, _Host host, Widget home) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  addTearDown(() => t.binding.setSurfaceSize(null));
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage()),
        transportFactoryProvider.overrideWithValue((_, _) async => host),
      ],
      child: MaterialApp(theme: atlasTheme(), home: home),
    ),
  );
  await _settle(t);
}

Future<void> _settle(WidgetTester t, [int ms = 300]) async {
  for (var i = 0; i < ms ~/ 50; i++) {
    await t.pump(const Duration(milliseconds: 50));
  }
}

Future<void> _unmount(WidgetTester t) async {
  await t.pumpWidget(const SizedBox());
  await _settle(t, 200);
}

/// The legal path a phone takes (resumed -> inactive -> hidden, and back through inactive).
void _background(WidgetTester t) {
  t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
  t.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
}

void _foreground(WidgetTester t) {
  t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
  t.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
}

/// The memory series the Metrics tab charts for `api`: one sample per reply the screen accepted.
List<double> _memSamples(WidgetTester t) =>
    t.widget<LinesChart>(find.byType(LinesChart)).series['api']!;

void main() {
  group('project panel', () {
    testWidgets(
      'a late reply to an older request does not roll the screen back or add a sample',
      (t) async {
        final host = _Host();
        await _pump(
          t,
          host,
          ProjectDetailScreen(device: _mac, project: _project),
        );
        host.reply('project_detail', 0, _detail(100));
        await _settle(t);
        await t.tap(find.text('Metrics'));
        await _settle(t);
        expect(_memSamples(t), [100]);

        // Two questions in flight at once, the way a slow link does it: a poll tick goes out and
        // stays unanswered, then the user pulls to refresh (a pull is never skipped, unlike a poll).
        await _settle(t, 3200);
        expect(host.pending('project_detail'), 1);
        await t.fling(
          find.byType(CustomScrollView),
          const Offset(0, 400),
          1000,
        );
        await _settle(t, 800);
        expect(host.pending('project_detail'), 2);

        // The newer answers first (300), then the older one lands late (200).
        host.reply('project_detail', 1, _detail(300));
        await _settle(t);
        host.reply('project_detail', 0, _detail(200));
        await _settle(t);
        expect(
          _memSamples(t),
          [100, 300],
          reason:
              'the late reply (200) must be dropped: no roll-back, no sample',
        );
        await _unmount(t);
      },
    );

    testWidgets(
      'polls ticking while one is in flight put ONE request on the wire',
      (t) async {
        final host = _Host();
        await _pump(
          t,
          host,
          ProjectDetailScreen(device: _mac, project: _project),
        );
        expect(host.sent('project_detail'), hasLength(1));
        // The first reply never comes. Three poll ticks (3 s each) pass.
        await _settle(t, 9500);
        expect(
          host.sent('project_detail'),
          hasLength(1),
          reason: 'a poll must not queue behind an unanswered request',
        );
        // It answers: the next tick polls again.
        host.reply('project_detail', 0, _detail(100));
        await _settle(t, 3200);
        expect(host.sent('project_detail'), hasLength(2));
        await _unmount(t);
      },
    );
  });

  group('polls rest while the app is in the background', () {
    testWidgets('the panel sends no project_detail until the app is back', (
      t,
    ) async {
      final host = _Host();
      await _pump(
        t,
        host,
        ProjectDetailScreen(device: _mac, project: _project),
      );
      host.reply('project_detail', 0, _detail(100));
      await _settle(t);
      expect(host.sent('project_detail'), hasLength(1));

      _background(t);
      await _settle(t, 9500);
      expect(
        host.sent('project_detail'),
        hasLength(1),
        reason: 'three poll ticks passed in the background',
      );

      _foreground(t);
      await _settle(t, 3200);
      expect(host.sent('project_detail'), hasLength(2));
      await _unmount(t);
    });

    testWidgets('the logs page sends no project_logs until the app is back', (
      t,
    ) async {
      final host = _Host();
      await _pump(
        t,
        host,
        ProjectDetailScreen(device: _mac, project: _project),
      );
      host.reply('project_detail', 0, _detail(100));
      await _settle(t);
      await t.tap(find.widgetWithText(TextButton, 'Logs').first);
      await _settle(t);
      host.reply('project_logs', 0, {'text': ''});
      await _settle(t);

      _background(t);
      await _settle(t, 9500);
      expect(host.sent('project_logs'), hasLength(1));

      _foreground(t);
      await _settle(t, 3200);
      expect(host.sent('project_logs'), hasLength(2));
      await _unmount(t);
    });
  });

  group('logs page', () {
    String logs(List<String> lines) =>
        lines.map((l) => '[api:1] 12:00:00 [stdout] $l').join('\n');

    /// The panel loaded, then its Logs button pushed the logs page.
    Future<_Host> openLogs(WidgetTester t) async {
      final host = _Host();
      await _pump(
        t,
        host,
        ProjectDetailScreen(device: _mac, project: _project),
      );
      host.reply('project_detail', 0, _detail(100));
      await _settle(t);
      await t.tap(find.widgetWithText(TextButton, 'Logs').first);
      await _settle(t);
      return host;
    }

    testWidgets('shows the lines the computer sends, and a read error', (
      t,
    ) async {
      final host = await openLogs(t);
      expect(host.sent('project_logs'), hasLength(1));
      host.reply('project_logs', 0, {
        'text': logs(['boot ok', 'listening']),
      });
      await _settle(t);
      expect(
        find.textContaining('boot ok', findRichText: true),
        findsOneWidget,
      );
      expect(
        find.textContaining('listening', findRichText: true),
        findsOneWidget,
      );
      await _unmount(t);
    });

    testWidgets('a slow reply for "a" does not overwrite the lines for "ab"', (
      t,
    ) async {
      final host = await openLogs(t);
      host.reply('project_logs', 0, {
        'text': logs(['first']),
      });
      await _settle(t);

      await t.enterText(find.byType(TextField), 'a');
      await t.testTextInput.receiveAction(TextInputAction.search);
      await _settle(t);
      await t.enterText(find.byType(TextField), 'ab');
      await t.testTextInput.receiveAction(TextInputAction.search);
      await _settle(t);
      final queries = host.sent('project_logs').map((a) => a['query']).toList();
      expect(queries, [null, 'a', 'ab']);

      // "ab" answers first; the reply for "a" lands late.
      host.reply('project_logs', 1, {
        'text': logs(['reply-for-ab']),
      });
      await _settle(t);
      host.reply('project_logs', 0, {
        'text': logs(['reply-for-a']),
      });
      await _settle(t);
      expect(
        find.textContaining('reply-for-ab', findRichText: true),
        findsOneWidget,
      );
      expect(
        find.textContaining('reply-for-a\n', findRichText: true),
        findsNothing,
      );
      expect(
        find.textContaining('reply-for-a', findRichText: true),
        findsOneWidget,
        reason:
            'only the "ab" row matches the prefix; the late "a" reply is gone',
      );
      await _unmount(t);
    });

    testWidgets('live polling does not queue behind an unanswered request', (
      t,
    ) async {
      final host = await openLogs(t);
      expect(host.sent('project_logs'), hasLength(1));
      await _settle(t, 9500);
      expect(host.sent('project_logs'), hasLength(1));
      host.reply('project_logs', 0, {
        'text': logs(['x']),
      });
      await _settle(t, 3200);
      expect(host.sent('project_logs'), hasLength(2));
      await _unmount(t);
    });

    testWidgets('a poll reply that lands after a filter change is dropped', (
      t,
    ) async {
      final host = await openLogs(t);
      host.reply('project_logs', 0, {
        'text': logs(['base']),
      });
      await _settle(t);
      // A live poll goes out and stays unanswered ...
      await _settle(t, 3200);
      expect(host.pending('project_logs'), 1);
      // ... the user picks the `api` service chip (a new question) ...
      await t.tap(find.widgetWithText(ChoiceChip, 'api'));
      await _settle(t);
      expect(host.pending('project_logs'), 2);
      host.reply('project_logs', 1, {
        'text': logs(['api-lines']),
      });
      await _settle(t);
      // ... and the stale all-services poll answers last.
      host.reply('project_logs', 0, {
        'text': logs(['all-services']),
      });
      await _settle(t);
      expect(
        find.textContaining('api-lines', findRichText: true),
        findsOneWidget,
      );
      expect(
        find.textContaining('all-services', findRichText: true),
        findsNothing,
      );
      await _unmount(t);
    });
  });
}
