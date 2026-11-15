import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';

import 'package:atlas_mobile/core/demo/demo_computer.dart';

import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/terminal/live_screen.dart';
import 'package:atlas_mobile/features/terminal/terminal_feed.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';
import 'package:xterm/xterm.dart';

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// What the readable feed shows: every block's command and output text.
String _screen(WidgetTester t) => [
  for (final w in t.widgetList<SelectableText>(find.byType(SelectableText)))
    w.textSpan!.toPlainText(),
  for (final w in t.widgetList<RichText>(find.byType(RichText)))
    w.text.toPlainText(),
].join('\n');

/// A computer whose pty output splits "é" (C3 A9) across two pushes, the way a
/// real pty read can cut a multi-byte character at a chunk boundary.
class _SplitUtf8 implements Transport {
  final _lines = StreamController<String>.broadcast();
  bool _closed = false;
  @override
  Stream<String> get incomingLines => _lines.stream;
  @override
  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    await _lines.close();
  }

  @override
  Future<void> sendLine(String line) async {
    final m = jsonDecode(line) as Map;
    if (m['cmd'] == 'pty_attach') {
      for (final chunk in [
        [0x63, 0x61, 0x66, 0xC3],
        [0xA9, 0x21],
      ]) {
        if (!_closed) {
          _lines.add(
            jsonEncode({
              'ev': 'pty_output',
              'pty': 'p',
              'data': base64.encode(chunk),
            }),
          );
        }
      }
    }
    scheduleMicrotask(() {
      if (!_closed) {
        _lines.add(
          jsonEncode({
            'id': m['id'],
            'ok': true,
            'data': m['cmd'] == 'info' ? {'proto': kAppProto} : {},
          }),
        );
      }
    });
  }
}

/// A transport that answers pty_info immediately but holds pty_attach until
/// [releaseAttach] is called. Used to verify the screen sends pty_detach when
/// disposed while pty_attach is still in flight.
class _DelayedAttach implements Transport {
  final _lines = StreamController<String>.broadcast();
  bool _closed = false;
  final List<String> sentLines = [];
  int? _attachId;

  @override
  Stream<String> get incomingLines => _lines.stream;

  @override
  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    await _lines.close();
  }

  /// Release the held pty_attach reply so its completer can resolve.
  void releaseAttach() {
    final id = _attachId;
    if (id != null && !_closed) {
      _lines.add(jsonEncode({'id': id, 'ok': true, 'data': {}}));
    }
  }

  @override
  Future<void> sendLine(String line) async {
    sentLines.add(line);
    final m = jsonDecode(line) as Map;
    final id = m['id'] as int;
    switch (m['cmd'] as String?) {
      case 'pty_info':
        scheduleMicrotask(() {
          if (!_closed) {
            _lines.add(
              jsonEncode({
                'id': id,
                'ok': true,
                'data': {'cols': 80, 'rows': 24},
              }),
            );
          }
        });
      case 'info':
        scheduleMicrotask(() {
          if (!_closed) {
            _lines.add(
              jsonEncode({
                'id': id,
                'ok': true,
                'data': {'proto': kAppProto},
              }),
            );
          }
        });
      case 'pty_attach':
        _attachId = id; // held until releaseAttach() is called
      case _:
        // pty_detach and anything else: acknowledge immediately.
        scheduleMicrotask(() {
          if (!_closed) {
            _lines.add(jsonEncode({'id': id, 'ok': true, 'data': {}}));
          }
        });
    }
  }
}

/// A computer that speaks enough of hostd for a terminal: `info`, `pty_info`, `pty_attach` (the
/// replay goes out BEFORE the reply, like hostd) and `pty_detach`. Everything it is sent is kept.
class _Computer implements Transport {
  _Computer(this.name);
  final String name;
  final _lines = StreamController<String>.broadcast();
  bool _closed = false;
  final List<Map<String, dynamic>> sent = [];

  @override
  Stream<String> get incomingLines => _lines.stream;

  int count(String cmd) => sent.where((m) => m['cmd'] == cmd).length;

  /// Output the program printed, as a `pty_output` push.
  void print(String text, {String pty = 'p'}) {
    if (_closed) return;
    _lines.add(
      jsonEncode({
        'ev': 'pty_output',
        'pty': pty,
        'data': base64.encode(utf8.encode(text)),
      }),
    );
  }

  /// The Mac sleeps with the socket still open: nothing is answered, nothing closes.
  bool muted = false;

  /// The link dies (the Mac sleeps, the network drops): the stream ends.
  Future<void> drop() => close();

  @override
  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    await _lines.close();
  }

  @override
  Future<void> sendLine(String line) async {
    final m = jsonDecode(line) as Map<String, dynamic>;
    // Recorded even when the link is down: a request aimed at a dead link is what is counted.
    sent.add(m);
    if (_closed) throw StateError('closed');
    final id = m['id'];
    Object data = {};
    switch (m['cmd']) {
      case 'info':
        data = {'proto': kAppProto};
      case 'pty_info':
        data = {'cols': 80, 'rows': 24};
      case 'pty_attach':
        print('replay from $name\r\n');
    }
    scheduleMicrotask(() {
      if (!_closed && !muted) {
        _lines.add(jsonEncode({'id': id, 'ok': true, 'data': data}));
      }
    });
  }
}

/// Opens [TerminalScreen] over a factory that hands out [dials] in order: a `_Computer`, or null
/// for "the computer cannot be reached". Dialling past the end of the list is unreachable too.
Future<ProviderContainer> _openDialling(
  WidgetTester t,
  List<_Computer?> dials,
) async {
  var n = 0;
  late ProviderContainer container;
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage()),
        transportFactoryProvider.overrideWithValue((_, _) async {
          final next = n < dials.length ? dials[n] : null;
          n++;
          if (next == null) throw StateError('unreachable');
          return next;
        }),
      ],
      child: Builder(
        builder: (context) {
          container = ProviderScope.containerOf(context);
          return MaterialApp(
            theme: atlasTheme(),
            home: TerminalScreen(device: demoDevice, pty: 'p', title: 't'),
          );
        },
      ),
    ),
  );
  return container;
}

/// Lets time pass in small steps, so the connection's backoff timer and its microtasks run.
Future<void> _settle(
  WidgetTester t, [
  Duration d = const Duration(seconds: 1),
]) async {
  for (var i = 0; i < d.inMilliseconds ~/ 50; i++) {
    await t.pump(const Duration(milliseconds: 50));
  }
}

/// What the grid of the terminal on screen holds.
String _grid(WidgetTester t) {
  final term = t.widget<TerminalFeed>(find.byType(TerminalFeed)).terminal;
  return term.buffer.lines.toList().map((l) => l.getText()).join('\n');
}

Future<void> _open(WidgetTester t, Transport Function() tr, String pty) async {
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage()),
        transportFactoryProvider.overrideWithValue((_, _) async => tr()),
      ],
      child: MaterialApp(
        theme: atlasTheme(),
        home: TerminalScreen(device: demoDevice, pty: pty, title: 'claude'),
      ),
    ),
  );
  for (var i = 0; i < 10; i++) {
    await t.pump(const Duration(milliseconds: 50));
  }
}

void main() {
  // Guards the blank agent terminal: hostd sends the replay before it answers
  // pty_attach, and the screen only started listening after the answer, so the
  // whole history was dropped. The demo transport keeps hostd's order.
  testWidgets('agent terminal shows the replay sent before the attach reply', (
    t,
  ) async {
    await _open(t, demoTransport, 'pty-demo-omp');
    expect(_screen(t), contains('~/Projetos/orbit-shop'));
    // The app bar names the agent, never the pty id.
    expect(find.text('claude'), findsOneWidget);
    expect(find.text('pty-demo-omp'), findsNothing);
    await t.pumpWidget(const SizedBox());
    await t.pump(const Duration(seconds: 3));
  });

  // Guards garbled accents: each push was decoded alone (String.fromCharCodes),
  // so a character split between two pushes came out as two wrong ones.
  testWidgets('a UTF-8 character split across two pushes renders intact', (
    t,
  ) async {
    await _open(t, _SplitUtf8.new, 'p');
    expect(_screen(t), contains('café!'));
    await t.pumpWidget(const SizedBox());
    await t.pump(const Duration(seconds: 3));
  });

  // Guards the cut-apart agent screen: the replay was drawn into the default
  // 80x24 grid, so a full-screen agent laid out for a 110-column Mac terminal
  // wrapped every row and its box broke into pieces.
  testWidgets('a full-screen agent is shown whole, at the Mac terminal size', (
    t,
  ) async {
    await _open(t, demoTransport, 'pty-demo-claude');
    expect(find.byType(LiveScreen), findsOneWidget, reason: 'automatic view');
    final term = t.widget<TerminalView>(find.byType(TerminalView)).terminal;
    expect((term.viewWidth, term.viewHeight), (110, 34));
    final top = term.buffer.lines[0].getText().trimRight();
    expect(top, startsWith('╭'));
    expect(top, endsWith('╮'), reason: 'the box border fits on one row');
    expect(top.length, 110);
    await t.pumpWidget(const SizedBox());
    await t.pump(const Duration(seconds: 3));
  });

  // Guards the pty leak on early dispose: if the widget is torn down while
  // pty_attach is still in flight (e.g. the user presses back immediately),
  // pty_detach must still be sent so the pty session does not leak on the Mac.
  testWidgets('no pty leak when widget is disposed before pty_attach replies', (
    t,
  ) async {
    final transport = _DelayedAttach();
    await t.pumpWidget(
      ProviderScope(
        overrides: [
          macStorageProvider.overrideWithValue(_Storage()),
          transportFactoryProvider.overrideWithValue((_, _) async => transport),
        ],
        child: MaterialApp(
          theme: atlasTheme(),
          home: TerminalScreen(device: demoDevice, pty: 'p', title: 'test'),
        ),
      ),
    );
    // Pump until pty_info has been answered (_mux set, _syncSize complete)
    // but pty_attach is still pending.
    for (var i = 0; i < 10; i++) {
      await t.pump(const Duration(milliseconds: 50));
    }
    expect(
      transport.sentLines.any((l) => l.contains('"cmd":"pty_attach"')),
      isTrue,
      reason: 'pty_attach must be in flight before we dispose',
    );
    // Dispose the widget while pty_attach is still in flight.
    await t.pumpWidget(const SizedBox());
    await t.pump(const Duration(milliseconds: 10));
    // dispose() fires pty_detach immediately via _mux (set before _syncSize).
    expect(
      transport.sentLines.any((l) => l.contains('"cmd":"pty_detach"')),
      isTrue,
      reason: 'pty_detach must be sent when widget is disposed mid-attach',
    );
    // Release the held pty_attach so no 20 s deadline timer lingers.
    transport.releaseAttach();
    await t.pump(const Duration(milliseconds: 10));
  });

  // Guards the frozen terminal after a drop: the screen re-attached only when the previous
  // connection value was a live mux, but a reconnect goes through `loading` first (the value is
  // dropped), so it kept the closed mux and listened to a dead stream, with no message.
  testWidgets('reconnect re-attaches the pty and draws the new output', (
    t,
  ) async {
    final first = _Computer('one'), second = _Computer('two');
    await _openDialling(t, [first, second]);
    await _settle(t);
    expect(first.count('pty_attach'), 1);
    expect(_grid(t), contains('replay from one'));

    await first.drop();
    await _settle(
      t,
      const Duration(seconds: 3),
    ); // backoff 1 s, then the new dial

    expect(
      second.count('pty_attach'),
      1,
      reason: 'attached again on the new link',
    );
    expect(_grid(t), contains('replay from two'));
    second.print('fresh output\r\n');
    await _settle(t);
    expect(_grid(t), contains('fresh output'));

    await t.pumpWidget(const SizedBox());
    await _settle(t, const Duration(seconds: 3));
  });

  // Guards the terminal that stays frozen after the app was in the background: past the 30 s
  // grace the connection settles on `data(null)` (no value), and the next foreground opens a new
  // mux from there, so the screen never saw a previous mux to replace and kept the closed one.
  testWidgets('re-attaches after the app was backgrounded past the grace', (
    t,
  ) async {
    final first = _Computer('one'), second = _Computer('two');
    final container = await _openDialling(t, [first, second]);
    await _settle(t);
    expect(first.count('pty_attach'), 1);
    final conn = container.read(
      connectionNotifierProvider(demoDevice).notifier,
    );

    conn.onBackgrounded();
    await _settle(t, const Duration(seconds: 31));
    expect(
      container.read(connectionNotifierProvider(demoDevice)).value,
      isNull,
      reason: 'the link was closed after the grace',
    );
    await conn.onForegrounded();
    await _settle(t, const Duration(seconds: 2));

    expect(second.count('pty_attach'), 1, reason: 'attached on the new link');
    expect(_grid(t), contains('replay from two'));

    await t.pumpWidget(const SizedBox());
    await _settle(t, const Duration(seconds: 3));
  });

  // Guards the screen that stays blank when it is opened while the computer is off: the first
  // attach found no connection, returned, and nothing ever attached once it came up.
  testWidgets(
    'opened while the computer is offline, it attaches when it comes up',
    (t) async {
      final up = _Computer('up');
      await _openDialling(t, [
        null,
        up,
      ]); // first dial fails, the retry succeeds
      await _settle(t, const Duration(milliseconds: 500));
      expect(
        find.textContaining('offline'),
        findsOneWidget,
        reason: 'says why it is blank',
      );
      expect(up.count('pty_attach'), 0);

      await _settle(t, const Duration(seconds: 3));

      expect(
        up.count('pty_attach'),
        1,
        reason: 'attached without reopening the screen',
      );
      expect(_grid(t), contains('replay from up'));
      expect(
        find.textContaining('offline'),
        findsNothing,
        reason: 'the notice goes away',
      );

      await t.pumpWidget(const SizedBox());
      await _settle(t, const Duration(seconds: 3));
    },
  );

  // Guards the detach going to the wrong link: after a reconnect it must go to the connection the
  // screen attached through now, never to the dropped one.
  testWidgets('dispose detaches on the connection it attached through', (
    t,
  ) async {
    final first = _Computer('one'), second = _Computer('two');
    await _openDialling(t, [first, second]);
    await _settle(t);
    await first.drop();
    await _settle(t, const Duration(seconds: 3));
    expect(second.count('pty_attach'), 1);

    await t.pumpWidget(const SizedBox());
    await _settle(t, const Duration(milliseconds: 200));

    expect(second.count('pty_detach'), 1);
    expect(first.count('pty_detach'), 0);
    await _settle(t, const Duration(seconds: 3));
  });

  // Guards the detach (and the re-attach) going to the wrong link when the old one is replaced
  // while it is still open: the phone wakes, the foreground probe finds the socket dead without it
  // ever closing, and a new link takes its place. The pty must be attached again through the new
  // one, and leaving the screen detaches there, not on the replaced link.
  testWidgets(
    'a link replaced while still open: re-attach and detach go to the new one',
    (t) async {
      final first = _Computer('one'), second = _Computer('two');
      final container = await _openDialling(t, [first, second]);
      await _settle(t);
      expect(first.count('pty_attach'), 1);
      final conn = container.read(
        connectionNotifierProvider(demoDevice).notifier,
      );
      conn.probeTimeout = const Duration(milliseconds: 200);

      first.muted = true; // asleep: the probe gets no answer
      // Not awaited: the probe's own timeout runs on the test clock, which only pumping moves.
      final probing = conn.onForegrounded();
      await _settle(t, const Duration(seconds: 3));
      await probing;

      expect(
        second.count('pty_attach'),
        1,
        reason: 'attached through the new link',
      );
      expect(_grid(t), contains('replay from two'));

      await t.pumpWidget(const SizedBox());
      await _settle(t, const Duration(milliseconds: 200));
      expect(second.count('pty_detach'), 1);
      expect(
        first.count('pty_detach'),
        0,
        reason: 'never sent to the replaced link',
      );
      await _settle(t, const Duration(seconds: 3));
    },
    timeout: const Timeout(Duration(minutes: 1)),
  );

  // Guards the poll that ran with no link: `pty_info` every 2 s whether or not there was one.
  testWidgets('no pty_info polling while disconnected', (t) async {
    final first = _Computer('one');
    await _openDialling(t, [first]); // every later dial fails: it stays down
    await _settle(t);
    await first.drop();
    final before = first.count('pty_info');
    await _settle(t, const Duration(seconds: 6));
    expect(
      first.count('pty_info'),
      before,
      reason: 'nothing sent to a dead link',
    );

    await t.pumpWidget(const SizedBox());
    await _settle(t, const Duration(seconds: 40));
  });
}
