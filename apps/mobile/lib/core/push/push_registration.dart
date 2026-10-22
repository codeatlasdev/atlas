import 'package:atlas_mobile/core/protocol/multiplexer.dart';

/// Returns true when the `info` response data indicates the host supports bots.
///
/// Pure function; easy to unit-test independently.
bool hostHasBotsCap(Map<String, dynamic>? data) =>
    (data?['caps'] as List?)?.contains('bots') == true;

/// Calls `info` on [mux] and returns whether the host has the `bots` cap.
Future<bool> checkBotsCap(Multiplexer mux) async {
  try {
    final resp = await mux.call('info').timeout(const Duration(seconds: 5));
    if (resp['ok'] != true) return false;
    return hostHasBotsCap(resp['data'] as Map<String, dynamic>?);
  } catch (_) {
    return false;
  }
}
