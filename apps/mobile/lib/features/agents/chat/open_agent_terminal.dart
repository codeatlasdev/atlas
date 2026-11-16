import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';

/// Opens the agent's own terminal: the live pty whose scope is [paneKey]. Says why in a
/// snack bar when the computer is out of reach or the agent has no open terminal.
Future<void> openAgentTerminal(
  BuildContext context,
  WidgetRef ref, {
  required MacDevice device,
  required String paneKey,
  required String title,
}) async {
  void say(String text) =>
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
  final Multiplexer mux;
  try {
    mux = await requireMuxOf(ref, device);
  } on DeviceUnavailable catch (e) {
    if (context.mounted) say(e.message);
    return;
  }
  final resp = await mux.call('pty_list');
  if (!context.mounted) return;
  if (resp['ok'] != true) {
    return say('${resp['error'] ?? 'Could not list terminals'}');
  }
  String? pty;
  for (final p in (resp['data']?['ptys'] as List?) ?? const []) {
    final m = p as Map;
    if (m['exited'] == null && (m['scope'] as Map?)?['pane_key'] == paneKey) {
      pty = m['pty'] as String?;
    }
  }
  if (pty == null) {
    return say('This agent has no open terminal');
  }
  context.push(
    '/terminal',
    extra: {'device': device, 'pty': pty, 'title': title, 'pane': paneKey},
  );
}
