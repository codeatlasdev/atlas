import 'dart:convert';
import 'dart:io';

/// The push relay the phone gets its grant from: the one crew servers notify through. Override
/// with `--dart-define=ATLAS_PUSH_RELAY=<url>` for a self-hosted relay (the crew servers need the
/// same value in their `ATLAS_PUSH_RELAY`).
const pushRelayUrl = String.fromEnvironment(
  'ATLAS_PUSH_RELAY',
  defaultValue: 'https://push.codeatlas.com.br',
);

/// Asks the relay for the grant of an FCM [token] (`POST /v1/register`). A grant proves nothing
/// about the phone; it is what crew servers must present to have the relay notify this token
/// (issue #205). `null` on any failure (offline, rate limited, relay too old): the caller retries
/// on its next connect.
Future<String?> fetchPushGrant(
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
    if (resp.statusCode != 200) return null;
    final grant = (jsonDecode(body) as Map<String, dynamic>)['grant'];
    return grant is String && grant.isNotEmpty ? grant : null;
  } catch (_) {
    return null;
  } finally {
    http.close(force: true);
  }
}
