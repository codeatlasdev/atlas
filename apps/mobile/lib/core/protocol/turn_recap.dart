/// What one finished turn did, for the recap card.
library;

import 'package:atlas_mobile/core/protocol/timeline_items.dart';

/// What one turn did, computed from its items alone.
class Recap {
  const Recap({
    this.files = const [],
    this.commands = 0,
    this.finished = 0,
    this.failed = 0,
    this.duration,
  });

  /// Distinct paths touched by `edit`/`write`/`ast_edit`, in order.
  final List<String> files;

  /// `bash` calls.
  final int commands;

  /// Subagents that finished well / badly (`result` items).
  final int finished;
  final int failed;
  final Duration? duration;

  bool get isEmpty =>
      files.isEmpty && commands == 0 && finished == 0 && failed == 0;

  static const _writers = {'edit', 'write', 'ast_edit'};

  factory Recap.of(List<TimelineItem> turn, {Duration? duration}) {
    final files = <String>{};
    var commands = 0, finished = 0, failed = 0;
    for (final i in turn) {
      if (i is ToolItem) {
        if (_writers.contains(i.name) && i.status != 'error') {
          final p = i.detail?.trim();
          if (p != null && p.isNotEmpty) files.add(p);
        } else if (i.name == 'bash') {
          commands++;
        }
      } else if (i is ResultItem) {
        i.status == 'completed' ? finished++ : failed++;
      }
    }
    return Recap(
      files: files.toList(),
      commands: commands,
      finished: finished,
      failed: failed,
      duration: duration,
    );
  }
}
