/// Text for the crew list: when a crewmate last spoke, what it said in one plain line, and
/// its job tag (ports of `fmtTime`, `roleTag` in `apps/desktop/src/lib/crew/`).
library;

/// How long ago [tsSeconds] was. `crewmate_list`'s `last.ts` is in SECONDS (hostd `now_s()`); read as
/// milliseconds it was 1970, so every row said "20748d".
String crewTimeAgo(int tsSeconds, DateTime now) {
  if (tsSeconds <= 0) return '';
  final at = DateTime.fromMillisecondsSinceEpoch(tsSeconds * 1000);
  final d = now.difference(at);
  if (d.inSeconds < 60) return 'now';
  if (d.inMinutes < 60) return '${d.inMinutes}m';
  if (d.inHours < 24) return '${d.inHours}h';
  if (d.inDays == 1) return 'yesterday';
  if (d.inDays < 7) return '${d.inDays}d';
  const months = [
    'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', //
    'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
  ];
  final day = '${at.day} ${months[at.month - 1]}';
  return at.year == now.year ? day : '$day ${at.year}';
}

/// Clock time of a task (`ts`, seconds), with the day when it is not today.
String crewChatTime(int tsSeconds, DateTime now) {
  if (tsSeconds <= 0) return '';
  final at = DateTime.fromMillisecondsSinceEpoch(tsSeconds * 1000);
  final hm =
      '${at.hour.toString().padLeft(2, '0')}:${at.minute.toString().padLeft(2, '0')}';
  final today = DateTime(now.year, now.month, now.day);
  final day = DateTime(at.year, at.month, at.day);
  final days = today.difference(day).inDays;
  if (days == 0) return hm;
  if (days == 1) return 'Yesterday $hm';
  final when = crewTimeAgo(tsSeconds, now);
  return days < 7 ? '${_weekday[at.weekday - 1]} $hm' : '$when, $hm';
}

const _weekday = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

const _monthName = [
  'January', 'February', 'March', 'April', 'May', 'June', //
  'July', 'August', 'September', 'October', 'November', 'December',
];

/// The local day of a task time (seconds), for grouping a thread by day.
DateTime crewDayOf(int tsSeconds) {
  final at = DateTime.fromMillisecondsSinceEpoch(tsSeconds * 1000);
  return DateTime(at.year, at.month, at.day);
}

/// The day separator above a day's first message: "Today, 12 November".
String crewDayLabel(int tsSeconds, DateTime now) {
  final day = crewDayOf(tsSeconds);
  final today = DateTime(now.year, now.month, now.day);
  final date =
      '${day.day} ${_monthName[day.month - 1]}'
      '${day.year == now.year ? '' : ' ${day.year}'}';
  return switch (today.difference(day).inDays) {
    0 => 'Today, $date',
    1 => 'Yesterday, $date',
    _ => date,
  };
}

/// Clock time inside a bubble (seconds): the day is on the separator above.
String crewClock(int tsSeconds) {
  final at = DateTime.fromMillisecondsSinceEpoch(tsSeconds * 1000);
  return '${at.hour.toString().padLeft(2, '0')}:${at.minute.toString().padLeft(2, '0')}';
}

/// How long a running task has been at it (`started_at`, seconds).
String crewElapsed(int startedSeconds, DateTime now) {
  final d = now.difference(
    DateTime.fromMillisecondsSinceEpoch(startedSeconds * 1000),
  );
  if (d.isNegative || d.inSeconds < 60) return '${d.inSeconds.clamp(0, 59)}s';
  if (d.inMinutes < 60) return '${d.inMinutes}m';
  return '${d.inHours}h ${d.inMinutes % 60}m';
}

/// A reply as one plain line: no markdown marks, links reduced to their text, bare URLs kept
/// short. The list showed raw `- **Estado:** #14 (https...`.
String crewPreview(String markdown) {
  var s = markdown;
  s = s.replaceAllMapped(RegExp(r'!?\[([^\]]*)\]\([^)]*\)'), (m) => m[1]!);
  s = s.replaceAll(RegExp(r'\(?https?://[^\s)]+\)?'), '');
  s = s.replaceAll(RegExp(r'```[a-z]*'), ' ');
  s = s.replaceAll(RegExp(r'(\*\*|__|\*|_|`|~~)'), '');
  s = s.replaceAll(
    RegExp(r'^\s*(#{1,6}\s+|>\s*|[-*+]\s+|\d+\.\s+)', multiLine: true),
    '',
  );
  s = s.replaceAll(RegExp(r'\s+'), ' ').trim();
  return s;
}

/// A short job tag from the role: the part before a colon (`Reviewer: checks PRs`), or a
/// CamelCase first word. Empty when the role has none.
String crewRoleTag(String role) {
  if (role.isEmpty) return '';
  final colon = role.indexOf(':');
  if (colon != -1) {
    final before = role.substring(0, colon).trim();
    if (before.isNotEmpty && before.length <= 20) return before;
  }
  final first = role.trim().split(RegExp(r'\s+')).first;
  if (first.length <= 15 &&
      RegExp(r'^[A-Z]').hasMatch(first) &&
      RegExp(r'[A-Z]').hasMatch(first.substring(1))) {
    return first;
  }
  return '';
}
