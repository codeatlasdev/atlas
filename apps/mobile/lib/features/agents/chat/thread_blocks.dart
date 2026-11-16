import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/agent_history.dart';
import 'package:atlas_mobile/core/protocol/timeline_blocks.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/timeline/timeline_block.dart';

/// The thread as the list draws it: its blocks, whether the typing bubble is under them
/// and whether older messages were cut.
class ChatThread {
  const ChatThread(this.blocks, this.typing, this.truncated);
  final List<TimelineBlock> blocks;
  final bool typing;
  final bool truncated;
}

/// `groupTimeline` of a history, and the widget built for each block, kept while their
/// inputs stay the same: a rebuild of the screen (status push, scroll flag, composer)
/// reuses them, so Flutter skips the unchanged blocks instead of laying out and
/// re-parsing every message again.
class ThreadBlocks {
  List<TimelineBlock>? _grouped;
  AgentHistory? _groupedOf;
  (bool, Duration?)? _groupedWith;
  final _built = <TimelineBlock, Widget>{};
  String? _builtParent;

  /// [groupTimeline] of [history], recomputed only when its inputs change.
  List<TimelineBlock> blocks(
    AgentHistory history, {
    required bool recapLast,
    required Duration? lastDuration,
  }) {
    final with_ = (recapLast, lastDuration);
    if (!identical(_groupedOf, history) || _groupedWith != with_) {
      _grouped = groupTimeline(
        history.items,
        recapLast: recapLast,
        lastDuration: lastDuration,
      );
      _groupedOf = history;
      _groupedWith = with_;
      _built.clear();
    }
    return _grouped!;
  }

  /// The padded widget of block [i] of [shown], built once per block. Blocks are the
  /// keys: identical blocks of a new grouping are new objects, so [blocks] clears this.
  Widget widgetAt(
    List<TimelineBlock> shown,
    int i,
    String? parentId, {
    required AgentHistory history,
    required void Function(String name) onOpenSubagent,
  }) {
    if (_builtParent != parentId) {
      _built.clear();
      _builtParent = parentId;
    }
    final b = shown[i];
    return _built[b] ??= Padding(
      padding: const EdgeInsets.only(top: Space.md),
      child: Builder(
        builder: (context) => timelineBlock(
          context,
          b,
          history: history,
          onOpenSubagent: onOpenSubagent,
          parentId: parentId,
        ),
      ),
    );
  }
}
