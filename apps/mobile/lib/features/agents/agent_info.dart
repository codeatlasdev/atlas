import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';

/// Agent states as the desktop labels them.
String agentStateLabel(String state) => switch (state) {
  'working' => 'working',
  'blocked' => 'needs you',
  'waiting' => 'asked you',
  'done' => 'done',
  _ => 'idle',
};

bool needsYou(AgentStatus a) => a.state == 'blocked' || a.state == 'waiting';

/// What to call an agent's session: its title, else the first prompt trimmed to a line,
/// else the agent's name.
String agentTitle(AgentStatus a, {String? historyTitle, String? firstPrompt}) {
  String line(String? s) {
    final t = (s ?? '').trim().replaceAll(RegExp(r'\s+'), ' ');
    return t.length > 60 ? '${t.substring(0, 60).trimRight()}…' : t;
  }

  for (final s in [historyTitle, a.title, firstPrompt, a.prompt]) {
    final t = line(s);
    if (t.isNotEmpty) return t;
  }
  return agentLabel(a.agent);
}

/// `3s`, `4m`, `2h 5m`: how long something has been going.
String elapsed(int sinceMs, {int? nowMs}) {
  final s =
      (((nowMs ?? DateTime.now().millisecondsSinceEpoch) - sinceMs) / 1000)
          .floor();
  if (sinceMs <= 0 || s < 0) return '';
  if (s < 60) return '${s}s';
  if (s < 3600) return '${s ~/ 60}m';
  final m = (s % 3600) ~/ 60;
  return m == 0 ? '${s ~/ 3600}h' : '${s ~/ 3600}h ${m}m';
}

/// States that can still produce new timeline items.
bool agentActive(String state) =>
    state == 'working' || state == 'blocked' || state == 'waiting';

/// `41s`, `3m 4s`, `1h 2m` from milliseconds; empty for none.
String durationText(int ms) {
  final s = ms ~/ 1000;
  if (ms <= 0) return '';
  if (s < 60) return '${s}s';
  if (s < 3600) return '${s ~/ 60}m ${s % 60}s';
  return '${s ~/ 3600}h ${(s % 3600) ~/ 60}m';
}

/// `48.2k`, `9.1k`, `812`.
String tokensText(int n) =>
    n >= 1000 ? '${(n / 1000).toStringAsFixed(n >= 100000 ? 0 : 1)}k' : '$n';

String costText(double usd) => usd <= 0
    ? ''
    : '\$${usd < 0.1 ? usd.toStringAsFixed(3) : usd.toStringAsFixed(2)}';

/// `292K`, `1M`, `812`: a context size.
String windowText(int n) {
  if (n >= 1000000) {
    final m = n / 1000000;
    return '${m == m.roundToDouble() ? m.toStringAsFixed(0) : m.toStringAsFixed(1)}M';
  }
  if (n >= 1000) return '${(n / 1000).round()}K';
  return '$n';
}

/// `12s ago`, `4m ago`; empty when [atMs] is unknown or in the future.
String agoText(int atMs, {int? nowMs}) {
  if (atMs <= 0) return '';
  final s = (((nowMs ?? DateTime.now().millisecondsSinceEpoch) - atMs) / 1000)
      .floor();
  if (s < 0) return '';
  if (s < 5) return 'just now';
  if (s < 60) return '${s}s ago';
  if (s < 3600) return '${s ~/ 60}m ago';
  if (s < 86400) return '${s ~/ 3600}h ago';
  return '${s ~/ 86400}d ago';
}

/// `HH:MM:SS` of an epoch-ms instant in local time; empty when unknown.
String clockText(int atMs) {
  if (atMs <= 0) return '';
  final d = DateTime.fromMillisecondsSinceEpoch(atMs);
  String two(int n) => n.toString().padLeft(2, '0');
  return '${two(d.hour)}:${two(d.minute)}:${two(d.second)}';
}
