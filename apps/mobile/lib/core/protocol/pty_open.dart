/// "New terminal": a shell on the computer, in a folder hostd resolves from ids (`pty_open`, `Act` scope).
///
/// The phone never sends a path or a program: `project` (and one of its `worktree` ids) on a computer, or on a
/// crew server nothing (the crew home) or a `crewmate` (its work tree). hostd starts the user's login shell there
/// and answers the terminal; typing into it stays behind the terminal screen's `term_write` scope and Face ID.
library;

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';

/// The `pty_open` reply: the terminal to attach.
class PtyOpened {
  const PtyOpened({required this.pty});
  final String pty;

  factory PtyOpened.fromJson(Map<String, dynamic> j) =>
      PtyOpened(pty: j['pty'] as String);
}

/// Whether [device] can open a terminal: not a crew server reached through a Mac, whose terminals are never
/// relayed (hostd refuses `pty_open` inside `crew_call`; pair the phone with the crew server itself).
bool canOpenTerminal(MacDevice device) => device.via == null;

/// Whether [device] is a crew server paired directly (its hostd lists the `crew` capability): it has no projects,
/// so a new terminal there starts in the crew home.
bool isCrewServer(MacDevice device) => device.caps.contains('crew');

/// Starts a shell on [device] (`pty_open`). Throws [StateError] with the server's own words on a refusal.
Future<PtyOpened> openTerminal(
  WidgetRef ref,
  MacDevice device, {
  String? project,
  String? worktree,
  String? crewmate,
}) async {
  final mux = await ref.read(connectionNotifierProvider(device).future);
  if (mux == null) throw StateError('${device.name} is offline');
  final resp = await mux.call('pty_open', {
    'project': ?project,
    'worktree': ?worktree,
    'crewmate': ?crewmate,
  });
  if (resp['ok'] != true) {
    throw StateError(resp['error'] as String? ?? 'the computer refused');
  }
  return PtyOpened.fromJson((resp['data'] as Map).cast<String, dynamic>());
}

/// [openTerminal], then the terminal screen (typing there is behind Face ID). A refusal is shown as a message and
/// nothing opens.
Future<void> openNewTerminal(
  BuildContext context,
  WidgetRef ref,
  MacDevice device, {
  String? project,
  String? worktree,
  String? crewmate,
  String? title,
}) async {
  final messenger = ScaffoldMessenger.of(context);
  final router = GoRouter.of(context);
  try {
    final opened = await openTerminal(
      ref,
      device,
      project: project,
      worktree: worktree,
      crewmate: crewmate,
    );
    await router.push<void>(
      '/terminal',
      extra: {
        'device': device,
        'pty': opened.pty,
        'title': title ?? 'Terminal',
      },
    );
  } catch (e) {
    final why = e is StateError ? e.message : '$e';
    messenger.showSnackBar(
      SnackBar(
        content: Text(
          why.startsWith('forbidden')
              ? 'This phone may not open terminals. Allow it on the computer (Paired phones).'
              : "Couldn't open a terminal: $why",
        ),
      ),
    );
  }
}
