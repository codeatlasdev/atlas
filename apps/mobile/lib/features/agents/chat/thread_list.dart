import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/chat_reader.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/chat/thread_blocks.dart';
import 'package:atlas_mobile/features/agents/chat/typing_bubble.dart';

/// The timeline of a chat: newest at the bottom, pull to refresh, swipe to the next
/// agent, the typing bubble under the last block and a button to the end when the reader
/// is away from it and news came in.
class ThreadList extends StatelessWidget {
  const ThreadList({
    super.key,
    required this.reader,
    required this.view,
    required this.empty,
    required this.blockAt,
    required this.onRefresh,
    required this.onSwipe,
  });

  final ChatReader<ChatThread> reader;

  /// The thread as the reader left it (see [ChatReader.view]).
  final ChatThread view;

  /// Loaded and nothing to show.
  final bool empty;

  /// The widget of block `i` of [view] (0 = the oldest).
  final Widget Function(int i) blockAt;
  final Future<void> Function() onRefresh;
  final void Function(double velocity) onSwipe;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final typing = view.typing;
    final slots =
        (typing ? 1 : 0) + view.blocks.length + (view.truncated ? 1 : 0);
    return Stack(
      children: [
        RefreshIndicator(
          color: c.accent,
          onRefresh: onRefresh,
          child: GestureDetector(
            behavior: HitTestBehavior.translucent,
            onHorizontalDragEnd: (d) => onSwipe(d.primaryVelocity ?? 0),
            child: ListView.builder(
              controller: reader.scroll,
              // Newest at the bottom, scrolled there on open.
              reverse: true,
              physics: const AlwaysScrollableScrollPhysics(),
              padding: const EdgeInsets.all(Space.lg),
              // Only the blocks near the viewport are built: a 200-item chat
              // costs what it shows.
              itemCount: slots,
              itemBuilder: (context, i) {
                if (typing && i == 0) {
                  return const Padding(
                    padding: EdgeInsets.only(top: Space.md),
                    child: Align(
                      alignment: Alignment.centerLeft,
                      child: TypingBubble(),
                    ),
                  );
                }
                final at = i - (typing ? 1 : 0);
                if (at < view.blocks.length) {
                  return blockAt(view.blocks.length - 1 - at);
                }
                return Padding(
                  padding: const EdgeInsets.only(top: Space.md),
                  child: Center(
                    child: Text(
                      'older messages not shown',
                      style: Theme.of(context).textTheme.labelSmall!
                          .copyWith(color: c.muted),
                    ),
                  ),
                );
              },
            ),
          ),
        ),
        if (empty)
          const EmptyState(
            title: 'Nothing here yet',
            message: 'What this agent does shows up here.',
          ),
        if (reader.hasNew)
          Positioned(
            bottom: Space.md,
            left: 0,
            right: 0,
            child: Center(
              child: NewActivityButton(onPressed: reader.jumpToEnd),
            ),
          ),
      ],
    );
  }
}
