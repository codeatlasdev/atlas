import 'dart:async';
import 'dart:convert';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:local_auth/local_auth.dart';
import 'package:xterm/xterm.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/terminal/agent_strip.dart';
import 'package:atlas_mobile/features/terminal/key_bar.dart';
import 'package:atlas_mobile/features/terminal/line_input.dart';
import 'package:atlas_mobile/features/terminal/live_screen.dart';
import 'package:atlas_mobile/features/terminal/terminal_feed.dart';
import 'package:atlas_mobile/features/terminal/terminal_theme.dart';

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

  /// Follows the Mac's terminal size while the screen is open and a link is up.
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

  /// Why the terminal is not attached right now (the computer is offline, speaks another
  /// protocol, or the attach was refused); null while attached or while still trying.
  String? _status;

  /// Counts attach attempts: an attempt that finds a newer one started while it waited stops,
  /// so two overlapping attaches never both subscribe and both send `pty_attach`.
  int _attempt = 0;

  /// Leaves the connection the pty was attached through: stops listening to it and polling it.
  /// Nothing is sent to it (it is gone, or about to be replaced).
  void _letGo() {
    _pushSub?.cancel();
    _pushSub = null;
    _sizeTimer?.cancel();
    _sizeTimer = null;
    _mux = null;
  }

  /// Attaches through the connection that is (or becomes) live. Runs on open and again on every
  /// new connection (see [build]); when the computer cannot be reached it says so and waits for
  /// the next live connection to try again.
  Future<void> _attach() async {
    final attempt = ++_attempt;
    final Multiplexer mux;
    try {
      mux = await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable catch (e) {
      if (mounted && attempt == _attempt) setState(() => _status = e.message);
      return;
    }
    if (!mounted || attempt != _attempt) return;
    _letGo();
    _mux = mux;
    if (_status != null) setState(() => _status = null);
    // Size the grid like the Mac's before the replay is drawn into it: hostd
    // sends the replay ahead of the attach reply, and a full-screen app drawn into
    // the default 80x24 came out cut apart.
    await _syncSize();
    // The widget may have been disposed (or the link replaced) while waiting for pty_info.
    if (!mounted || attempt != _attempt) return;
    _sizeTimer = Timer.periodic(const Duration(seconds: 2), (_) => _syncSize());
    // Listen first: hostd sends the replay before it answers pty_attach, so a
    // listener added after the reply misses the whole screen (it showed blank).
    _pushSub = mux.pushEvents.listen(
      _onPush,
      // The link ended: stop polling it. The next live link re-attaches (see [build]).
      onDone: () {
        if (identical(mux, _mux)) _letGo();
      },
    );
    final resp = await mux.call('pty_attach', {
      'pty': widget.pty,
      'replay': true,
      'output': true,
    });
    // The widget may have been disposed while pty_attach was in flight.
    if (!mounted) {
      mux.call('pty_detach', {'pty': widget.pty}).ignore();
      return;
    }
    // The link was replaced meanwhile: the new attempt owns the screen now.
    if (attempt != _attempt) return;
    if (resp['ok'] != true) {
      _terminal.write(
        '\x1b[31m${resp['error'] ?? 'could not attach'}\x1b[0m\r\n',
      );
    }
  }

  void _onPush(Map<String, dynamic> event) {
    final ev = event['ev'] as String?;
    if (event['pty'] != widget.pty) return;
    switch (ev) {
      case 'pty_output':
      case 'pty_replay':
        _utf8.add(base64.decode(event['data'] as String));
      case 'pty_title':
        if (mounted) setState(() => _title = event['title'] as String? ?? '');
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

  /// Reads the Mac's terminal size through the attached link; does nothing without one, so the
  /// 2 s poll costs nothing while the computer is off.
  Future<void> _syncSize() async {
    final mux = _mux;
    if (mux == null) return;
    final resp = await mux.call('pty_info', {'pty': widget.pty});
    final d = resp['data'];
    // Stale: the screen let go of this link (or was closed) while the call was out.
    if (!mounted || !identical(mux, _mux) || d is! Map) return;
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
    final Multiplexer mux;
    try {
      mux = await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable catch (e) {
      return _toast(e.message);
    }
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

  /// The line above the terminal when it is not following the Mac: why (offline, protocol) or
  /// that the link is being reopened. Null while attached.
  String? _notice(AsyncValue<Multiplexer?> conn) {
    if (_status != null) return _status;
    // Reopening: the old (closed) link is still the notifier's value until the new one answers.
    if (conn.isLoading) return 'Reconnecting…';
    return null;
  }

  @override
  Widget build(BuildContext context) {
    // The connection, watched so the notice above the terminal follows it.
    final conn = ref.watch(connectionNotifierProvider(widget.device));
    // Hold the connection open while this screen is up, and re-attach whenever
    // a new mux arrives (reconnect after a drop or 30 s background close).
    ref.listen(connectionNotifierProvider(widget.device), (prev, next) {
      // While the link reopens, the state still carries the OLD (closed) mux as its value:
      // only a settled state names the link that is live now.
      if (next.isLoading) return;
      final mux = next.value;
      if (mux != null) {
        // A live link that is not the one we attached through: re-attach on it.
        if (!identical(mux, _mux)) _attach();
      } else {
        // Settled with no link (the computer is off, or speaks another protocol): stop polling a
        // dead link and say why the screen has stopped moving.
        _letGo();
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
            AgentStrip(device: widget.device, pane: widget.pane!),
          if (_notice(conn) case final text?) _LinkNotice(text),
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
              padding: const EdgeInsets.symmetric(
                vertical: Space.sm,
                horizontal: Space.lg,
              ),
              child: Text(
                'The process exited',
                style: TextStyle(color: c.muted),
              ),
            )
          else if (_writeEnabled) ...[
            KeyBar(onKey: _sendRaw, enabled: true),
            LineInput(onSubmit: (t) => _sendRaw('$t\r')),
          ] else
            SafeArea(
              top: false,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(
                  Space.md,
                  Space.sm,
                  Space.md,
                  Space.sm,
                ),
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

/// One line above the terminal: why it is not following the Mac right now.
class _LinkNotice extends StatelessWidget {
  const _LinkNotice(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      color: c.raised,
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        vertical: Space.sm,
        horizontal: Space.lg,
      ),
      child: Text(text, style: TextStyle(color: c.muted)),
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
