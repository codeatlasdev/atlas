import 'dart:async';

import 'package:flutter/foundation.dart';

import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/push/push_result.dart';

/// Whether the `info` response data says the host takes `push_register`: a crew server (`crew`, its
/// crewmates push) or a hostd that pushes about its own agents (`push`, every hostd from this change
/// on). An older plain hostd has neither and would answer an unknown command.
///
/// Pure function; easy to unit-test independently.
bool hostCanPush(Map<String, dynamic>? data) {
  final caps = data?['caps'] as List?;
  return caps?.contains('crew') == true || caps?.contains('push') == true;
}

/// Calls `info` on [mux] and says whether the host takes `push_register` ([hostCanPush]); a
/// [PushFail] names the reason (too old, refused, no answer).
Future<PushResult<void>> checkCanPush(Multiplexer mux) async {
  try {
    final resp = await mux.call('info').timeout(const Duration(seconds: 5));
    if (resp['ok'] != true) {
      return PushFail(
        'The computer refused info: ${resp['error'] ?? 'unknown'}',
      );
    }
    if (hostCanPush(resp['data'] as Map<String, dynamic>?)) {
      return const PushOk(null);
    }
    return const PushFail(
      'This computer is too old to send notifications: update Atlas on it',
    );
  } on TimeoutException {
    return const PushFail('The computer did not answer');
  } catch (e) {
    return PushFail('The computer could not be asked: $e');
  }
}

/// Gets the relay's grant for an FCM [token]. Injectable: tests answer it without a network, the
/// app uses [fetchPushGrant].
typedef PushGrantFetcher = Future<PushResult<String>> Function(String token);

/// Registers [token] with the computer behind [mux]: asks the relay for the token's grant, then
/// `push_register {token, platform, grant}` (the crew server presents the grant on every notify;
/// without it the relay answers 403, issue #205). A [PushFail] says which step failed and why.
///
/// With no grant nothing is registered: the server would keep an entry the relay can never accept.
/// The next connect tries again.
Future<PushResult<void>> registerPush(
  Multiplexer mux,
  String token,
  PushGrantFetcher fetchGrant,
) async {
  try {
    final grant = await fetchGrant(token);
    if (grant is PushFail<String>) return PushFail(grant.reason);
    final resp = await mux.call('push_register', {
      'token': token,
      'platform': 'fcm',
      'grant': (grant as PushOk<String>).value,
    });
    if (resp['ok'] == true) return const PushOk(null);
    return PushFail(
      'The computer refused the registration: ${resp['error'] ?? 'unknown'}',
    );
  } catch (e) {
    debugPrint('atlas-mobile: push_register failed: $e');
    return PushFail('Registering with the computer failed: $e');
  }
}
