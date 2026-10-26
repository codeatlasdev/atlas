import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:local_auth/local_auth.dart';
import 'package:xterm/xterm.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart'
    show agentStateLabel;
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/terminal/live_screen.dart';
import 'package:atlas_mobile/features/terminal/terminal_feed.dart';

/// Full-screen terminal view for a single PTY session.
///
/// - Read-only by default; biometric auth unlocks input.
/// - Key bar sends raw escape sequences; never sends pty_resize.
/// - Detaches from the PTY on dispose.
class TerminalScreen extends ConsumerStatefulWidget {
  const TerminalScreen({
    super.key,
    required this.device,
    required this.pty,
    this.title,
    this.pane,
  });

  final MacDevice device;
  final String pty;

  /// What the app bar shows until the program sets a title (never the pty id).
  final String? title;

  /// The agent's pane, when this is an agent's terminal: shows what it is doing.
  final String? pane;

  @override
  ConsumerState<TerminalScreen> createState() => _TerminalScreenState();
}

class _TerminalScreenState extends ConsumerState<TerminalScreen> {
  final Terminal _terminal = Terminal();
  StreamSubscription<Map<String, dynamic>>? _pushSub;

  /// Output arrives in chunks that can split a UTF-8 character; decode as a stream
  /// straight into the terminal.
  late final _utf8 = const Utf8Decoder(allowMalformed: true)
      .startChunkedConversion(_TerminalSink(_terminal));

  bool _writeEnabled = false;

  /// The view the user picked; null = automatic (full-screen app → live
  /// screen, shell → feed).
  _View? _picked;

  /// Follows the Mac's terminal size while the screen is open.
  Timer? _sizeTimer;
  String _title = '';
  bool _exited = false;

  @override
  void initState() {
    super.initState();
    // Defer attach until after first build so ref is ready.
    WidgetsBinding.instance.addPostFrameCallback((_) => _attach());
  }

  /// The connection the pty is attached through.
  Multiplexer? _mux;

  Future<void> _attach() async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null || !mounted) return;
    _mux = mux;
    // Size the grid like the Mac's before the replay is drawn into it: hostd
    // sends the replay ahead of the attach reply, and a full-screen app drawn into
    // the default 80x24 came out cut apart.
    await _syncSize();
    // The widget may have been disposed while waiting for pty_info.
    if (!mounted) return;
    _sizeTimer = Timer.periodic(const Duration(seconds: 2), (_) => _syncSize());
    // Listen first: hostd sends the replay before it answers pty_attach, so a
    // listener added after the reply misses the whole screen (it showed blank).
    _pushSub = mux.pushEvents.listen(_onPush);
    final resp = await _sendAttach(mux);
    // The widget may have been disposed while pty_attach was in flight.
    if (!mounted) {
      mux.call('pty_detach', {'pty': widget.pty}).ignore();
      return;
    }
    if (resp['ok'] != true) {
      _terminal.write(
        '\x1b[31m${resp['error'] ?? 'could not attach'}\x1b[0m\r\n',
      );
    }
  }

  Future<Map<String, dynamic>> _sendAttach(Multiplexer mux) => mux.call(
    'pty_attach',
    {'pty': widget.pty, 'replay': true, 'output': true},
  );

  void _onPush(Map<String, dynamic> event) {
    final ev = event['ev'] as String?;
    if (event['pty'] != widget.pty) return;
    switch (ev) {
      case 'pty_replay':
        // The start of a replay: the screen is rebuilt from it (a no-op on the first attach, the
        // clean slate after a reattach), not appended to what was there.
        // Through the same sink as the output, so it lands in order; xterm.dart ignores RIS (ESC c).
        _utf8.add(utf8.encode('\x1b[H\x1b[2J\x1b[3J'));
        _utf8.add(base64.decode(event['data'] as String));
      case 'pty_output':
        _utf8.add(base64.decode(event['data'] as String));
      case 'pty_title':
        if (mounted) setState(() => _title = event['title'] as String? ?? '');
      case 'pty_dropped':
        // hostd stopped feeding this phone because it did not read fast enough: the replay
        // (its last 2 MiB) redraws what was missed, then live output goes on.
        final mux = _mux;
        if (mux != null && mounted) _sendAttach(mux).ignore();
      case 'pty_exit':
        if (mounted) setState(() => _exited = true);
    }
  }

  @override
  void dispose() {
    _pushSub?.cancel();
    _sizeTimer?.cancel();
    // Fire-and-forget detach on the connection we attached through (`ref` is
    // unusable here); errors mean the connection is already gone.
    _mux?.call('pty_detach', {'pty': widget.pty}).ignore();
    super.dispose();
  }

  Future<void> _syncSize() async {
    final resp = await _mux?.call('pty_info', {'pty': widget.pty});
    final d = resp?['data'];
    if (!mounted || d is! Map) return;
    final cols = d['cols'], rows = d['rows'];
    if (cols is int &&
        rows is int &&
        cols > 0 &&
        rows > 0 &&
        (cols != _terminal.viewWidth || rows != _terminal.viewHeight)) {
      setState(() => _terminal.resize(cols, rows));
    }
  }

  _View get _view =>
      _picked ?? (_terminal.isUsingAltBuffer ? _View.screen : _View.feed);

  Future<void> _toggleWrite() async {
    if (_writeEnabled) {
      setState(() => _writeEnabled = false);
      return;
    }
    try {
      final ok = await LocalAuthentication().authenticate(
        localizedReason: 'Type into this terminal on your Mac',
      );
      if (ok && mounted) setState(() => _writeEnabled = true);
    } catch (e) {
      _toast('Face ID unavailable: $e');
    }
  }

  void _toast(String text) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
  }

  Future<void> _sendRaw(String data) async {
    if (!_writeEnabled) return;
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return _toast('Not connected to your Mac');
    final resp = await mux.call('pty_write', {
      'pty': widget.pty,
      'data': base64.encode(utf8.encode(data)),
    });
    if (resp['ok'] != true) {
      final err = resp['error'] as String? ?? 'error';
      _toast(
        err.startsWith('forbidden')
            ? 'This phone may not type in terminals. Allow it on your Mac (Devices).'
            : err,
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    // Hold the connection open while this screen is up, and re-attach whenever
    // a new mux arrives (reconnect after a drop or 30 s background close).
    ref.listen(connectionNotifierProvider(widget.device), (prev, next) {
      final prevMux = prev?.value;
      final nextMux = next.value;
      if (nextMux != null && prevMux != null && !identical(prevMux, nextMux)) {
        // Cancel the old subscription and timer before re-attaching.
        _pushSub?.cancel();
        _pushSub = null;
        _sizeTimer?.cancel();
        _sizeTimer = null;
        _mux = null;
        _attach();
      }
    });
    final titleStr = _title.isNotEmpty ? _title : (widget.title ?? 'Terminal');
    final c = AtlasColors.of(context);
    return Scaffold(
      backgroundColor: atlasTerminalTheme.background,
      appBar: AppBar(
        backgroundColor: atlasTerminalTheme.background,
        title: Text(titleStr, overflow: TextOverflow.ellipsis),
        actions: [
          // The character grid exactly as on the Mac, for when the feed's
          // reflow gets a full-screen app wrong.
          IconButton(
            icon: Icon(
              _view == _View.screen
                  ? Icons.view_agenda_outlined
                  : Icons.fit_screen_rounded,
            ),
            tooltip: _view == _View.screen ? 'Readable view' : 'Whole screen',
            onPressed: () => setState(
              () => _picked = _view == _View.screen ? _View.feed : _View.screen,
            ),
          ),
          if (_writeEnabled)
            IconButton(
              icon: Icon(Icons.lock_open_rounded, color: c.warning),
              tooltip: 'Stop typing',
              onPressed: _toggleWrite,
            ),
        ],
      ),
      body: Column(
        children: [
          if (widget.pane != null)
            _AgentStrip(device: widget.device, pane: widget.pane!),
          Expanded(
            // Rebuilt on output so the automatic view follows the program into
            // and out of full screen.
            child: ListenableBuilder(
              listenable: _TerminalListenable(_terminal),
              builder: (_, _) => _view == _View.screen
                  ? LiveScreen(terminal: _terminal, theme: atlasTerminalTheme)
                  : TerminalFeed(
                      terminal: _terminal,
                      theme: atlasTerminalTheme,
                    ),
            ),
          ),
          if (_exited)
            Container(
              color: c.raised,
              width: double.infinity,
              padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 16),
              child: Text(
                'The process exited',
                style: TextStyle(color: c.muted),
              ),
            )
          else if (_writeEnabled) ...[
            _KeyBar(onKey: _sendRaw, enabled: true),
            _LineInput(onSubmit: (t) => _sendRaw('$t\r')),
          ] else
            SafeArea(
              top: false,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
                child: SizedBox(
                  width: double.infinity,
                  child: FilledButton.tonalIcon(
                    onPressed: _toggleWrite,
                    icon: const Icon(Icons.keyboard_rounded),
                    label: const Text('Tap to type'),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

enum _View { feed, screen }

/// Adapts xterm's listener API to [Listenable], for [ListenableBuilder].
class _TerminalListenable implements Listenable {
  _TerminalListenable(this.t);
  final Terminal t;
  @override
  void addListener(VoidCallback l) => t.addListener(l);
  @override
  void removeListener(VoidCallback l) => t.removeListener(l);
}

/// What the agent on this terminal is doing, from its hooks: state and the task
/// it is on, above its screen.
class _AgentStrip extends ConsumerWidget {
  const _AgentStrip({required this.device, required this.pane});
  final MacDevice device;
  final String pane;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final a = ref.watch(agentStatusMapNotifierProvider(device)).value?[pane];
    if (a == null) return const SizedBox.shrink();
    final c = AtlasColors.of(context);
    final k = c.agentState(a.state);
    final task = a.state == 'working' ? a.prompt : (a.lastMessage ?? a.prompt);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(14, 8, 14, 10),
      decoration: BoxDecoration(
        color: c.raised,
        border: Border(bottom: BorderSide(color: c.borderSubtle)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              StatusDot(color: k, pulse: a.state == 'working', size: 6),
              Text(
                agentStateLabel(a.state),
                style: TextStyle(
                  color: k,
                  fontSize: 12,
                  fontWeight: FontWeight.w700,
                ),
              ),
            ],
          ),
          if (task != null && task.trim().isNotEmpty) ...[
            const SizedBox(height: 3),
            Text(
              task.replaceAll('\n', ' '),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: c.text, fontSize: 13, height: 1.3),
            ),
          ],
        ],
      ),
    );
  }
}

/// Feeds decoded text into the terminal.
class _TerminalSink implements Sink<String> {
  _TerminalSink(this.terminal);
  final Terminal terminal;
  @override
  void add(String s) => terminal.write(s);
  @override
  void close() {}
}

// ── Key bar ───────────────────────────────────────────────────────────────────

class _KeyBar extends StatefulWidget {
  const _KeyBar({required this.onKey, required this.enabled});

  final void Function(String) onKey;
  final bool enabled;

  @override
  State<_KeyBar> createState() => _KeyBarState();
}

class _KeyBarState extends State<_KeyBar> {
  bool _ctrlActive = false;

  void _tap(String seq) {
    if (widget.enabled) widget.onKey(seq);
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final bg = c.base;

    Widget key(String label, String seq) => Padding(
      padding: const EdgeInsets.symmetric(horizontal: 3, vertical: 6),
      child: TextButton(
        style: TextButton.styleFrom(
          minimumSize: const Size(44, 36),
          padding: const EdgeInsets.symmetric(horizontal: 10),
          backgroundColor: c.raised,
          foregroundColor: c.text,
          disabledForegroundColor: c.faint,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(Space.radiusSm),
          ),
          textStyle: const TextStyle(
            fontFamily: monoFamily,
            fontFamilyFallback: monoFallback,
            fontSize: 13,
          ),
        ),
        onPressed: widget.enabled ? () => _tap(seq) : null,
        child: Text(label),
      ),
    );

    final baseKeys = [
      key('Esc', '\x1b'),
      key('Tab', '\t'),
      // Ctrl toggle
      TextButton(
        style: TextButton.styleFrom(
          minimumSize: const Size(44, 36),
          padding: const EdgeInsets.symmetric(horizontal: 10),
          backgroundColor: _ctrlActive ? c.accent : c.raised,
          foregroundColor: _ctrlActive ? c.onAccent : c.text,
          disabledForegroundColor: c.faint,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(Space.radiusSm),
          ),
        ),
        onPressed: widget.enabled
            ? () => setState(() => _ctrlActive = !_ctrlActive)
            : null,
        child: const Text('Ctrl'),
      ),
      key('←', '\x1b[D'),
      key('↑', '\x1b[A'),
      key('↓', '\x1b[B'),
      key('→', '\x1b[C'),
    ];

    // When ctrl is active, show A–Z row instead of base keys.
    final ctrlKeys = [
      for (final c in 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split(''))
        key('^$c', String.fromCharCode(c.codeUnitAt(0) - 64)),
    ];

    return Container(
      decoration: BoxDecoration(
        color: bg,
        border: Border(top: BorderSide(color: c.borderSubtle)),
      ),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
        child: Row(children: _ctrlActive ? ctrlKeys : baseKeys),
      ),
    );
  }
}

/// One line typed with the system keyboard, sent with Enter (the key bar covers Esc,
/// Tab, Ctrl and arrows).
class _LineInput extends StatefulWidget {
  const _LineInput({required this.onSubmit});
  final void Function(String) onSubmit;

  @override
  State<_LineInput> createState() => _LineInputState();
}

class _LineInputState extends State<_LineInput> {
  final _c = TextEditingController();

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  void _send() {
    widget.onSubmit(_c.text);
    _c.clear();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      color: c.base,
      padding: EdgeInsets.fromLTRB(
        Space.md,
        Space.sm,
        Space.sm,
        // The home indicator, unless the keyboard is up and already below us.
        MediaQuery.viewInsetsOf(context).bottom > 0
            ? Space.sm
            : MediaQuery.paddingOf(context).bottom,
      ),
      child: Row(
        children: [
          Expanded(
            child: TextField(
              controller: _c,
              autocorrect: false,
              enableSuggestions: false,
              textCapitalization: TextCapitalization.none,
              style: const TextStyle(
                fontFamily: monoFamily,
                fontFamilyFallback: monoFallback,
                fontSize: 14,
              ),
              decoration: const InputDecoration(hintText: 'Type, then return'),
              onSubmitted: (_) => _send(),
              textInputAction: TextInputAction.send,
            ),
          ),
          IconButton(
            tooltip: 'Send line',
            icon: const Icon(Icons.keyboard_return_rounded),
            onPressed: _send,
          ),
        ],
      ),
    );
  }
}

/// The desktop's Atlas Dark terminal colours (atlas.json `terminal.*`).
const atlasTerminalTheme = TerminalTheme(
  cursor: Color(0xFFC0CAF5),
  selection: Color(0xFF243A61),
  foreground: Color(0xFFC0CAF5),
  background: Color(0xFF131C2B),
  black: Color(0xFF15161E),
  red: Color(0xFFF7768E),
  green: Color(0xFF9ECE6A),
  yellow: Color(0xFFE0AF68),
  blue: Color(0xFF7AA2F7),
  magenta: Color(0xFFBB9AF7),
  cyan: Color(0xFF7DCFFF),
  white: Color(0xFFA9B1D6),
  brightBlack: Color(0xFF414868),
  brightRed: Color(0xFFF7768E),
  brightGreen: Color(0xFF9ECE6A),
  brightYellow: Color(0xFFE0AF68),
  brightBlue: Color(0xFF7AA2F7),
  brightMagenta: Color(0xFFBB9AF7),
  brightCyan: Color(0xFF7DCFFF),
  brightWhite: Color(0xFFC0CAF5),
  searchHitBackground: Color(0x40E6B46D),
  searchHitBackgroundCurrent: Color(0x73EF945F),
  searchHitForeground: Color(0xFFC0CAF5),
);
