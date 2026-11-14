/// The reads and writes behind the crew operations screens (audit, usage, engine limits, crew
/// settings, routines). Each call goes to the crew server of [MacDevice] (relayed through the Mac for a
/// crew server behind one) and throws with the server's own message when it refuses.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
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
  final mux = await ref.watch(connectionNotifierProvider(device).future);
  if (mux == null) throw StateError('${device.name} is offline');
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
  final mux = await ref.read(connectionNotifierProvider(device).future);
  if (mux == null) throw StateError('${device.name} is offline');
  final resp = await mux.call('crew_settings_set', settings.toArgs());
  if (resp['ok'] != true) {
    throw StateError(resp['error'] as String? ?? 'the server refused');
  }
  ref.invalidate(crewSettingsProvider(device));
}

/// Replaces a crewmate's routines with [routines] (`crewmate_routine_set`, `Act` scope: hostd keeps the
/// whole list it is sent, and refuses the lot when one cron is invalid).
Future<void> saveRoutines(
  WidgetRef ref,
  MacDevice device,
  String crewmateId,
  List<Routine> routines,
) async {
  final mux = await ref.read(connectionNotifierProvider(device).future);
  if (mux == null) throw StateError('${device.name} is offline');
  final resp = await mux.call('crewmate_routine_set', {
    'id': crewmateId,
    'routines': [for (final r in routines) r.toJson()],
  });
  if (resp['ok'] != true) {
    throw StateError(resp['error'] as String? ?? 'the server refused');
  }
  ref.invalidate(crewProvider(device));
}
