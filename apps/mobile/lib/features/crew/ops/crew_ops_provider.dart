/// The reads and writes behind the crew operations screens (audit, usage, engine limits, crew
/// settings, routines). Each call goes to the crew server of [MacDevice] (relayed through the Mac for a
/// crew server behind one) and throws with the server's own message when it refuses.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';

/// `cmd` on [device]'s connection; the `data` of the reply, or a [StateError] with the reason.
Future<Map<String, dynamic>> callCrew(
  Ref ref,
  MacDevice device,
  String cmd, [
  Map<String, dynamic> args = const {},
]) async {
  final mux = await requireMux(ref, device);
  final resp = await mux.call(cmd, args);
  if (resp['ok'] != true) {
    throw StateError(resp['error'] as String? ?? 'the server refused');
  }
  return (resp['data'] as Map?)?.cast<String, dynamic>() ?? const {};
}

/// How many entries one audit read asks for (hostd caps what it sends too).
const auditPageSize = 200;

/// Which log to read: one crewmate's, or the crew's own (`crewmateId` null).
typedef AuditScope = ({MacDevice device, String? crewmateId});

/// The audit of [AuditScope], newest first (`crewmate_audit` / `crew_audit`).
final auditProvider = FutureProvider.autoDispose
    .family<List<AuditEntry>, AuditScope>((ref, scope) async {
      final id = scope.crewmateId;
      final data = await callCrew(
        ref,
        scope.device,
        id == null ? 'crew_audit' : 'crewmate_audit',
        {'id': ?id, 'limit': auditPageSize},
      );
      return parseAudit(data);
    });

/// A crewmate's token usage records (`crewmate_usage`), all time (hostd keeps what it was sent).
final usageProvider = FutureProvider.autoDispose
    .family<List<UsageRecord>, ({MacDevice device, String crewmateId})>((
      ref,
      key,
    ) async {
      final data = await callCrew(ref, key.device, 'crewmate_usage', {
        'id': key.crewmateId,
        'since': 0,
      });
      return parseUsage(data);
    });

/// What the engine CLIs expose about their limits (`engine_limits`).
final engineLimitsProvider = FutureProvider.autoDispose
    .family<EngineLimits, MacDevice>((ref, device) async {
      return EngineLimits.fromJson(
        await callCrew(ref, device, 'engine_limits'),
      );
    });

/// The crew-wide settings of a crew server (`crew_settings_get`).
final crewSettingsProvider = FutureProvider.autoDispose
    .family<CrewSettings, MacDevice>((ref, device) async {
      return CrewSettings.fromJson(
        await callCrew(ref, device, 'crew_settings_get'),
      );
    });

/// Saves [settings] (`crew_settings_set`, `Act` scope) and re-reads them.
Future<void> saveCrewSettings(
  WidgetRef ref,
  MacDevice device,
  CrewSettings settings,
) async {
  final mux = await requireMuxOf(ref, device);
  final resp = await mux.call('crew_settings_set', settings.toArgs());
  if (resp['ok'] != true) {
    throw StateError(resp['error'] as String? ?? 'the server refused');
  }
  ref.invalidate(crewSettingsProvider(device));
  // Bypass off moves crewmates out of it: their rows are read again.
  ref.invalidate(crewProvider(device));
}

/// Whether each engine of [device] is signed in (`crew_engine_status` with `refresh`: hostd runs each engine's
/// own status command, bounded, so the row is the truth when the screen opens and after a sign-in).
final engineStatusProvider = FutureProvider.autoDispose
    .family<List<EngineStatus>, MacDevice>((ref, device) async {
      return parseEngineStatus(
        await callCrew(ref, device, 'crew_engine_status', {'refresh': true}),
      );
    });

/// Starts the sign-in terminal of [engine] on [device] (`crew_engine_login`, `Act` scope): the terminal runs only
/// that engine's login command, and the caller opens it (`TerminalScreen`, which keeps typing behind Face ID).
Future<EngineLogin> startEngineLogin(
  WidgetRef ref,
  MacDevice device,
  String engine,
) async {
  final mux = await requireMuxOf(ref, device);
  final resp = await mux.call('crew_engine_login', {'engine': engine});
  if (resp['ok'] != true) {
    throw StateError(resp['error'] as String? ?? 'the server refused');
  }
  ref.invalidate(engineStatusProvider(device));
  return EngineLogin.fromJson((resp['data'] as Map).cast<String, dynamic>());
}

/// Replaces a crewmate's routines with [routines] (`crewmate_routine_set`, `Act` scope: hostd keeps the
/// whole list it is sent, and refuses the lot when one cron is invalid).
Future<void> saveRoutines(
  WidgetRef ref,
  MacDevice device,
  String crewmateId,
  List<Routine> routines,
) async {
  final mux = await requireMuxOf(ref, device);
  final resp = await mux.call('crewmate_routine_set', {
    'id': crewmateId,
    'routines': [for (final r in routines) r.toJson()],
  });
  if (resp['ok'] != true) {
    throw StateError(resp['error'] as String? ?? 'the server refused');
  }
  ref.invalidate(crewProvider(device));
}
