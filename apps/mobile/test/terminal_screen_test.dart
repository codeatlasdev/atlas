import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';

import 'fixtures/demo_computer.dart';

import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/terminal/live_screen.dart';
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
        _lines.add(jsonEncode({'id': m['id'], 'ok': true, 'data': {}}));
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
}
