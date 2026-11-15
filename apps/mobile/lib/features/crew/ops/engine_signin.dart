/// "Sign in here": starts the sign-in terminal of one engine on a crew server and opens it. The one path
/// the settings rows and the task action card share. The terminal runs only that engine's login command
/// (`crew_engine_login`, `Act` scope); typing into it stays behind the terminal screen's Face ID gate.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/crew/ops/crew_ops_provider.dart';

/// Opens the sign-in terminal of [engine] on [device]. A refusal (read-only phone, engine not installed,
/// server offline) is shown as the server's own message; nothing opens.
Future<void> openEngineSignin(
  BuildContext context,
  WidgetRef ref,
  MacDevice device,
  String engine,
) async {
  final messenger = ScaffoldMessenger.of(context);
  final router = GoRouter.of(context);
  try {
    final login = await startEngineLogin(ref, device, engine);
    await router.push<void>(
      '/terminal',
      extra: {
        'device': device,
        'pty': login.pty,
        'title': 'Sign in: ${engineLabels[engine] ?? engine}',
      },
    );
    // Back from the terminal: the row asks the engine again.
    ref.invalidate(engineStatusProvider(device));
  } catch (e) {
    final why = e is StateError ? e.message : '$e';
    messenger.showSnackBar(
      SnackBar(
        content: Text(
          why.startsWith('forbidden')
              ? 'This phone may not start a sign-in. Allow it on the crew server (Devices).'
              : "Couldn't open the sign-in: $why",
        ),
      ),
    );
  }
}
