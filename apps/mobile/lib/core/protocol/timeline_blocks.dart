/// Timeline items grouped for display: tool runs folded, turns split.
library;

import 'package:atlas_mobile/core/protocol/timeline_items.dart';
import 'package:atlas_mobile/core/protocol/turn_recap.dart';

/// What the timeline renders: user/assistant/thinking items as they are, and each run of
/// consecutive plain tool calls folded into one [ToolGroup]; `task` calls stay apart as
/// [SubagentsBlock]s.
sealed class TimelineBlock {
  const TimelineBlock();
}

class ItemBlock extends TimelineBlock {
  const ItemBlock(this.item);
  final TimelineItem item;
}

class ToolGroup extends TimelineBlock {
  const ToolGroup(this.calls);
  final List<ToolItem> calls;

  bool get running => calls.any((c) => c.status == 'running');
  int get errors => calls.where((c) => c.status == 'error').length;

  /// Distinct tool names in order of first use: `bash, read, edit`.
  List<String> get names => {for (final c in calls) c.name}.toList();
}

class SubagentsBlock extends TimelineBlock {
  const SubagentsBlock(this.call);
  final ToolItem call;
}

/// The summary card at the top of a finished turn.
class RecapBlock extends TimelineBlock {
  const RecapBlock(this.recap);
  final Recap recap;
}

/// Groups [items] for display. With [recapLast] the last turn also gets its recap
/// (the agent finished it); earlier turns always do, as they are over.
List<TimelineBlock> groupTimeline(
  List<TimelineItem> items, {
  bool recapLast = false,
  Duration? lastDuration,
}) {
  final out = <TimelineBlock>[];
  final run = <ToolItem>[];
  void flush() {
    if (run.isEmpty) return;
    out.add(ToolGroup(List.of(run)));
    run.clear();
  }

  final turns = splitTurns(items);
  // A turn's tool run never continues into the next turn: its user item flushes it.
  for (var t = 0; t < turns.length; t++) {
    final turn = turns[t];
    final last = t == turns.length - 1;
    final recap = last && !recapLast
        ? null
        : Recap.of(turn, duration: last ? lastDuration : null);
    for (var k = 0; k < turn.length; k++) {
      final i = turn[k];
      if (i is ToolItem && !i.isTask) {
        run.add(i);
        continue;
      }
      flush();
      out.add(i is ToolItem ? SubagentsBlock(i) : ItemBlock(i));
      // The recap sits right under the prompt that started the turn.
      if (k == 0 && i is UserItem && recap != null && !recap.isEmpty) {
        out.add(RecapBlock(recap));
      }
    }
  }
  flush();
  return out;
}

/// [items] cut into turns: each starts at a [UserItem] (the first may start without).
List<List<TimelineItem>> splitTurns(List<TimelineItem> items) {
  final turns = <List<TimelineItem>>[];
  for (final i in items) {
    if (i is UserItem || turns.isEmpty) turns.add([]);
    turns.last.add(i);
  }
  return turns;
}
