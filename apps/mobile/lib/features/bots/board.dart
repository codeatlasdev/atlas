/// Pure board logic — port of apps/desktop/src/lib/bots/board.ts
///
/// [boardColumns] maps flat task entries (one per bot+task) into four sorted,
/// filtered columns. No Flutter/UI dependencies: fully testable in dart:test.
library;

import 'package:atlas_mobile/core/protocol/models.dart';

// ── types ─────────────────────────────────────────────────────────────────────

class BoardEntry {
  const BoardEntry({
    required this.hostKey,
    required this.bot,
    required this.task,
  });
  final String hostKey;
  final BotListEntry bot;
  final BotTask task;
}

class BoardCard extends BoardEntry {
  const BoardCard({
    required super.hostKey,
    required super.bot,
    required super.task,
    this.title,
    this.failed = false,
    this.cancelled = false,
  });

  /// The agent's name for the work; null until omp sets it or for unnamed tasks.
  final String? title;

  /// Task ended in an error.
  final bool failed;

  /// Task was cancelled by the user.
  final bool cancelled;

  BoardCard withTitle(String? t) => BoardCard(
    hostKey: hostKey,
    bot: bot,
    task: task,
    title: t,
    failed: failed,
    cancelled: cancelled,
  );
  BoardCard withFailed() => BoardCard(
    hostKey: hostKey,
    bot: bot,
    task: task,
    title: title,
    failed: true,
    cancelled: false,
  );
  BoardCard withCancelled() => BoardCard(
    hostKey: hostKey,
    bot: bot,
    task: task,
    title: title,
    failed: false,
    cancelled: true,
  );
}

enum ColumnId { queued, working, needsYou, done }

class BoardColumn {
  const BoardColumn({
    required this.id,
    required this.label,
    required this.cards,
  });
  final ColumnId id;
  final String label;
  final List<BoardCard> cards;
}

// ── filter options ─────────────────────────────────────────────────────────────

class BoardFilterOptions {
  const BoardFilterOptions({
    this.bots,
    this.source,
    this.query,
    this.doneDays = 1,
  });

  /// Only show cards from these bots ("$hostKey\x00$botId"). Null or empty = all.
  final Set<String>? bots;

  /// 'chat' = desktop/mobile only; 'auto' = non-chat only; null/'all' = no filter.
  final String? source;

  /// Case-insensitive substring match on heading (title ?? prompt).
  final String? query;

  /// How many days back to show in Done. 1 = calendar today (default); 7 = rolling 7 days.
  final int doneDays;
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// True when [ts] (ms) falls within the same calendar day as [now] (local time).
bool _isToday(int ts, int now) {
  final d = DateTime.fromMillisecondsSinceEpoch(ts);
  final n = DateTime.fromMillisecondsSinceEpoch(now);
  return d.year == n.year && d.month == n.month && d.day == n.day;
}

/// True when [ts] is within the last [days] × 24 h from [now] (rolling window).
bool _isWithinDays(int ts, int now, int days) =>
    now - ts <= days * 24 * 60 * 60 * 1000;

/// Timestamp to use for "when did this task end" — prefer finishedAt over ts.
int _endTs(BotTask task) => task.finishedAt ?? task.ts;

// ── boardColumns ──────────────────────────────────────────────────────────────

/// Partition [entries] into four columns and sort each:
///   Queued    — oldest first (lowest ts)
///   Working   — longest-running first (lowest startedAt / ts)
///   Needs you — oldest first (lowest ts)
///   Done      — newest first (highest finishedAt / ts); calendar today by default
///
/// A card is a task the agent named itself (title); one card per bot×title holds
/// the latest message about that work. A finished unnamed chat message stays off;
/// an unnamed one in flight, or one an automation started, is shown under its prompt.
List<BoardColumn> boardColumns(
  List<BoardEntry> entries,
  int nowMs, {
  BoardFilterOptions? opts,
}) {
  final o = opts ?? const BoardFilterOptions();
  final q = o.query?.toLowerCase();

  // ── pre-dedup entry filters ────────────────────────────────────────────────
  var filtered = entries;
  if (o.bots != null && o.bots!.isNotEmpty) {
    filtered = filtered
        .where((e) => o.bots!.contains('${e.hostKey}\x00${e.bot.id}'))
        .toList();
  }
  if (o.source == 'chat') {
    filtered = filtered
        .where((e) => e.task.source == 'desktop' || e.task.source == 'mobile')
        .toList();
  } else if (o.source == 'auto') {
    filtered = filtered
        .where((e) => e.task.source != 'desktop' && e.task.source != 'mobile')
        .toList();
  }

  final queued = <BoardCard>[];
  final working = <BoardCard>[];
  final needsYou = <BoardCard>[];
  final done = <BoardCard>[];

  // ── deduplication ──────────────────────────────────────────────────────────
  // Keep only the latest task per (hostKey, botId, title) key.
  final latest = <String, BoardCard>{};

  for (final e in filtered) {
    final task = e.task;
    final title = task.title?.trim();
    if (title == null || title.isEmpty) {
      final isChat = task.source == 'desktop' || task.source == 'mobile';
      final finished =
          task.state == 'done' ||
          task.state == 'failed' ||
          task.state == 'cancelled';
      if (isChat && finished) continue;
      latest['${e.hostKey}\x00${e.bot.id}\x00#${task.id}'] = BoardCard(
        hostKey: e.hostKey,
        bot: e.bot,
        task: task,
      );
      continue;
    }
    final key = '${e.hostKey}\x00${e.bot.id}\x00$title';
    final prev = latest[key];
    if (prev == null || task.ts >= prev.task.ts) {
      latest[key] = BoardCard(
        hostKey: e.hostKey,
        bot: e.bot,
        task: task,
        title: title,
      );
    }
  }

  // ── column assignment ──────────────────────────────────────────────────────
  for (final card in latest.values) {
    final task = card.task;
    final bot = card.bot;

    // Query filter runs post-dedup so it matches the resolved title.
    if (q != null && !(card.title ?? task.prompt).toLowerCase().contains(q)) {
      continue;
    }

    final inDoneWindow = o.doneDays == 1
        ? _isToday(_endTs(task), nowMs)
        : _isWithinDays(_endTs(task), nowMs, o.doneDays);

    switch (task.state) {
      case 'queued':
        queued.add(card);
      case 'running':
        if (bot.pendingApproval != null) {
          needsYou.add(card);
        } else {
          working.add(card);
        }
      case 'waiting_action':
        needsYou.add(card);
      case 'done':
        if (inDoneWindow) done.add(card);
      case 'failed':
        if (inDoneWindow) done.add(card.withFailed());
      case 'cancelled':
        if (inDoneWindow) done.add(card.withCancelled());
    }
  }

  queued.sort((a, b) => a.task.ts.compareTo(b.task.ts));
  working.sort(
    (a, b) => (a.task.startedAt ?? a.task.ts).compareTo(
      b.task.startedAt ?? b.task.ts,
    ),
  );
  needsYou.sort((a, b) => a.task.ts.compareTo(b.task.ts));
  done.sort((a, b) => _endTs(b.task).compareTo(_endTs(a.task)));

  return [
    BoardColumn(id: ColumnId.queued, label: 'Queued', cards: queued),
    BoardColumn(id: ColumnId.working, label: 'Working', cards: working),
    BoardColumn(id: ColumnId.needsYou, label: 'Needs you', cards: needsYou),
    BoardColumn(id: ColumnId.done, label: 'Done', cards: done),
  ];
}
