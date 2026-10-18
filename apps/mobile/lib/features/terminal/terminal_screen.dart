import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:local_auth/local_auth.dart';
import 'package:xterm/xterm.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';

/// Full-screen terminal view for a single PTY session.
///
/// - Read-only by default; biometric auth unlocks input.
/// - Key bar sends raw escape sequences; never sends pty_resize.
/// - Detaches from the PTY on dispose.
class TerminalScreen extends ConsumerStatefulWidget {
  const TerminalScreen({super.key, required this.device, required this.pty});

  final MacDevice device;
  final String pty;

  @override
  ConsumerState<TerminalScreen> createState() => _TerminalScreenState();
}

class _TerminalScreenState extends ConsumerState<TerminalScreen> {
  final Terminal _terminal = Terminal();
  final TerminalController _ctrl = TerminalController();
  StreamSubscription<Map<String, dynamic>>? _pushSub;

  bool _writeEnabled = false;
  String _title = '';
  bool _exited = false;

  @override
  void initState() {
    super.initState();
    // Defer attach until after first build so ref is ready.
    WidgetsBinding.instance.addPostFrameCallback((_) => _attach());
  }

  Future<void> _attach() async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null || !mounted) return;
    await mux.call('pty_attach', {
      'pty': widget.pty,
      'replay': true,
      'output': true,
    });
    _pushSub = mux.pushEvents.listen(_onPush);
  }

  void _onPush(Map<String, dynamic> event) {
    final ev = event['ev'] as String?;
    if (event['pty'] != widget.pty) return;
    switch (ev) {
      case 'pty_output':
      case 'pty_replay':
        final bytes = base64.decode(event['data'] as String);
        _terminal.write(String.fromCharCodes(bytes));
      case 'pty_title':
        if (mounted) setState(() => _title = event['title'] as String? ?? '');
      case 'pty_exit':
        if (mounted) setState(() => _exited = true);
    }
  }

  @override
  void dispose() {
    _pushSub?.cancel();
    // Fire-and-forget detach; ignore errors (connection may already be gone).
    ref.read(connectionNotifierProvider(widget.device)).value?.call(
      'pty_detach',
      {'pty': widget.pty},
    ).ignore();
    _ctrl.dispose();
    super.dispose();
  }

  Future<void> _toggleWrite() async {
    if (_writeEnabled) {
      setState(() => _writeEnabled = false);
      return;
    }
    final auth = LocalAuthentication();
    try {
      final ok = await auth.authenticate(
        localizedReason: 'Enable terminal write access',
      );
      if (ok && mounted) setState(() => _writeEnabled = true);
    } catch (_) {
      // Biometric not available — deny silently.
    }
  }

  void _sendRaw(String data) {
    if (!_writeEnabled) return;
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    mux?.call('pty_write', {
      'pty': widget.pty,
      'data': base64.encode(utf8.encode(data)),
    }).ignore();
  }

  @override
  Widget build(BuildContext context) {
    final titleStr = _title.isEmpty ? widget.pty : _title;
    return Scaffold(
      backgroundColor: atlasTerminalTheme.background,
      appBar: AppBar(
        backgroundColor: atlasTerminalTheme.background,
        title: Text(titleStr, overflow: TextOverflow.ellipsis),
        actions: [
          IconButton(
            icon: Icon(
              _writeEnabled ? Icons.lock_open_rounded : Icons.lock_rounded,
              color: _writeEnabled ? AtlasColors.of(context).warning : null,
            ),
            tooltip: _writeEnabled
                ? 'Disable terminal input'
                : 'Enable terminal input',
            onPressed: _toggleWrite,
          ),
        ],
      ),
      body: Column(
        children: [
          Expanded(
            child: TerminalView(
              _terminal,
              controller: _ctrl,
              // Input goes through the line field and key bar below, so the
              // system keyboard never covers half of a wide TUI uninvited.
              readOnly: true,
              theme: atlasTerminalTheme,
              textStyle: const TerminalStyle(
                fontSize: 12,
                fontFamily: monoFamily,
              ),
              backgroundOpacity: 1,
            ),
          ),
          if (_exited)
            Container(
              color: AtlasColors.of(context).raised,
              width: double.infinity,
              padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 16),
              child: Text(
                'The process exited',
                style: TextStyle(color: AtlasColors.of(context).muted),
              ),
            ),
          AnimatedSize(
            duration: Motion.normal,
            curve: Motion.ease,
            child: _writeEnabled && !_exited
                ? _LineInput(onSubmit: (t) => _sendRaw('$t\r'))
                : const SizedBox(width: double.infinity),
          ),
          _KeyBar(onKey: _sendRaw, enabled: _writeEnabled),
        ],
      ),
    );
  }
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
      child: SafeArea(
        top: false,
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
          child: Row(children: _ctrlActive ? ctrlKeys : baseKeys),
        ),
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
      padding: const EdgeInsets.fromLTRB(Space.md, Space.sm, Space.sm, 0),
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
