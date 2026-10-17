import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
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
  const TerminalScreen({
    super.key,
    required this.device,
    required this.pty,
  });

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
    final mux =
        await ref.read(connectionNotifierProvider(widget.device).future);
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
    ref
        .read(connectionNotifierProvider(widget.device))
        .value
        ?.call('pty_detach', {'pty': widget.pty})
        .ignore();
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
    final mux =
        ref.read(connectionNotifierProvider(widget.device)).value;
    mux
        ?.call('pty_write', {
          'pty': widget.pty,
          'data': base64.encode(utf8.encode(data)),
        })
        .ignore();
  }

  @override
  Widget build(BuildContext context) {
    final titleStr = _title.isEmpty ? widget.pty : _title;
    return Scaffold(
      appBar: AppBar(
        title: Text(titleStr),
        actions: [
          IconButton(
            icon: Icon(_writeEnabled ? Icons.lock_open : Icons.lock),
            tooltip:
                _writeEnabled ? 'Disable terminal input' : 'Enable terminal input',
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
              // Input is handled exclusively by the key bar; the widget is
              // always "read-only" so the on-screen keyboard never appears.
              readOnly: true,
            ),
          ),
          if (_exited)
            Container(
              color: Colors.grey.shade300,
              width: double.infinity,
              padding: const EdgeInsets.symmetric(vertical: 6, horizontal: 12),
              child: const Text(
                'Process exited',
                style: TextStyle(color: Colors.black54),
              ),
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
    final bg = Theme.of(context).colorScheme.surfaceContainerHighest;

    Widget key(String label, String seq) => TextButton(
          style: TextButton.styleFrom(
            minimumSize: const Size(44, 40),
            padding: const EdgeInsets.symmetric(horizontal: 8),
          ),
          onPressed: widget.enabled ? () => _tap(seq) : null,
          child: Text(label),
        );

    final baseKeys = [
      key('Esc', '\x1b'),
      key('Tab', '\t'),
      // Ctrl toggle
      TextButton(
        style: TextButton.styleFrom(
          minimumSize: const Size(44, 40),
          padding: const EdgeInsets.symmetric(horizontal: 8),
          backgroundColor:
              _ctrlActive ? Theme.of(context).colorScheme.primary : null,
          foregroundColor:
              _ctrlActive ? Theme.of(context).colorScheme.onPrimary : null,
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
      color: bg,
      child: SafeArea(
        top: false,
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
          child: Row(
            children: _ctrlActive ? ctrlKeys : baseKeys,
          ),
        ),
      ),
    );
  }
}
