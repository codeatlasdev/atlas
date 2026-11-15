import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:atlas_mobile/core/push/push_result.dart';

/// The push relay the phone gets its grant from: the one crew servers notify through. Override
/// with `--dart-define=ATLAS_PUSH_RELAY=<url>` for a self-hosted relay (the crew servers need the
/// same value in their `ATLAS_PUSH_RELAY`).
const pushRelayUrl = String.fromEnvironment(
  'ATLAS_PUSH_RELAY',
  defaultValue: 'https://push.codeatlas.com.br',
);

/// Asks the relay for the grant of an FCM [token] (`POST /v1/register`). A grant proves nothing
/// about the phone; it is what crew servers must present to have the relay notify this token
/// (issue #205). A [PushFail] says why (offline, rate limited, relay down or too old): the caller
/// shows it and retries on its next connect.
Future<PushResult<String>> fetchPushGrant(
  String token, {
  String relay = pushRelayUrl,
  Duration timeout = const Duration(seconds: 10),
}) async {
  final http = HttpClient()..connectionTimeout = timeout;
  try {
    final req = await http
        .postUrl(
          Uri.parse('${relay.replaceFirst(RegExp(r'/+$'), '')}/v1/register'),
        )
        .timeout(timeout);
    req.headers.contentType = ContentType.json;
    req.write(jsonEncode({'token': token}));
    final resp = await req.close().timeout(timeout);
    final body = await resp.transform(utf8.decoder).join().timeout(timeout);
    final status = resp.statusCode;
    if (status != 200) return PushFail(_relayRefusal(status));
    try {
      final grant = (jsonDecode(body) as Map<String, dynamic>)['grant'];
      if (grant is String && grant.isNotEmpty) return PushOk(grant);
    } catch (_) {
      // Falls through to the same answer as a body without a grant.
    }
    return const PushFail(
      'The push relay sent an unexpected answer (is it an older version?)',
    );
  } on TimeoutException {
    return const PushFail('The push relay did not answer in time');
  } on SocketException {
    return const PushFail(
      'The push relay could not be reached (check the connection)',
    );
  } catch (e) {
    return PushFail('The push relay could not be reached ($e)');
  } finally {
    http.close(force: true);
  }
}

String _relayRefusal(int status) => switch (status) {
  429 => 'The push relay says too many requests (HTTP 429); it retries on the next connect',
  400 || 401 || 403 => 'The push relay refused this phone (HTTP $status)',
  >= 500 => 'The push relay is down (HTTP $status)',
  _ => 'The push relay answered HTTP $status',
};
