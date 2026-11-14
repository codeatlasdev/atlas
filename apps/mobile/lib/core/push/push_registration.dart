import 'package:flutter/foundation.dart';

import 'package:atlas_mobile/core/protocol/multiplexer.dart';

/// Returns true when the `info` response data indicates the host supports crew.
///
/// Pure function; easy to unit-test independently.
bool hostHasCrewCap(Map<String, dynamic>? data) =>
    (data?['caps'] as List?)?.contains('crew') == true;

/// Calls `info` on [mux] and returns whether the host has the `crew` cap.
Future<bool> checkCrewCap(Multiplexer mux) async {
  try {
    final resp = await mux.call('info').timeout(const Duration(seconds: 5));
    if (resp['ok'] != true) return false;
    return hostHasCrewCap(resp['data'] as Map<String, dynamic>?);
  } catch (_) {
    return false;
  }
}

/// Gets the relay's grant for an FCM [token] (`null` when it cannot be had). Injectable: tests
/// answer it without a network, the app uses [fetchPushGrant].
typedef PushGrantFetcher = Future<String?> Function(String token);

/// Registers [token] with the computer behind [mux]: asks the relay for the token's grant, then
/// `push_register {token, platform, grant}` (the crew server presents the grant on every notify;
/// without it the relay answers 403, issue #205). Returns whether the server accepted it.
///
/// With no grant nothing is registered: the server would keep an entry the relay can never accept.
/// The next connect tries again.
Future<bool> registerPush(
  Multiplexer mux,
  String token,
  PushGrantFetcher fetchGrant,
) async {
  try {
    final grant = await fetchGrant(token);
    if (grant == null || grant.isEmpty) return false;
    final resp = await mux.call('push_register', {
      'token': token,
      'platform': 'fcm',
      'grant': grant,
    });
    return resp['ok'] == true;
  } catch (e) {
    debugPrint('atlas-mobile: push_register failed: $e');
    return false;
  }
}
