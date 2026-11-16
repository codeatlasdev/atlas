import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/agent_history.dart';
import 'package:atlas_mobile/core/protocol/timeline_blocks.dart';
import 'package:atlas_mobile/core/protocol/timeline_items.dart';
import 'package:atlas_mobile/features/agents/timeline/chat_messages.dart';
import 'package:atlas_mobile/features/agents/timeline/recap_card.dart';
import 'package:atlas_mobile/features/agents/timeline/subagents_card.dart';
import 'package:atlas_mobile/features/agents/timeline/timeline_notices.dart';
import 'package:atlas_mobile/features/agents/timeline/tool_cards.dart';

/// One timeline block, in order. [onOpenSubagent] focuses an agent by id (`''` = main);
/// an IRC card from `parent` opens [parentId], the focused agent's parent (the main agent
/// when null).
Widget timelineBlock(
  BuildContext context,
  TimelineBlock b, {
  required AgentHistory history,
  required void Function(String name) onOpenSubagent,
  String? parentId,
}) => switch (b) {
  ItemBlock(item: UserItem(:final text)) => UserBubble(text),
  ItemBlock(item: AssistantItem(:final text)) => AssistantText(text),
  ItemBlock(item: ThinkingItem(:final text)) => ThinkingRow(text),
  ItemBlock(item: final ResultItem r) => ResultCard(r, onOpen: onOpenSubagent),
  ItemBlock(item: CompactionItem(:final text)) => CompactionDivider(text),
  ItemBlock(item: final IrcItem i) => IrcCard(
    i,
    onOpen: i.from == 'parent' ? () => onOpenSubagent(parentId ?? '') : null,
  ),
  ItemBlock(item: NoticeItem(:final text)) => NoticeRow(text),
  ItemBlock(item: final AdvisorItem a) => AdvisorCard(a),
  ItemBlock() => const SizedBox.shrink(),
  ToolGroup() => ToolGroupCard(b),
  SubagentsBlock(:final call) => SubagentsCard(
    call: call,
    history: history,
    onOpen: onOpenSubagent,
  ),
  RecapBlock(:final recap) => RecapCard(recap),
};
