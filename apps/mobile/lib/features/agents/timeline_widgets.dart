import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/markdown/atlas_markdown.dart';

/// Copies [text] on long-press, with a haptic tick and a short confirmation.
Future<void> copyText(BuildContext context, String text) async {
  HapticFeedback.selectionClick();
  await Clipboard.setData(ClipboardData(text: text));
  if (!context.mounted) return;
  ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(
      const SnackBar(
        content: Text('Copied'),
        duration: Duration(milliseconds: 1200),
      ),
    );
}

/// What the user said: a bubble on the right.
class UserBubble extends StatelessWidget {
  const UserBubble(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    const r = Radius.circular(18);
    return Align(
      alignment: Alignment.centerRight,
      child: GestureDetector(
        onLongPress: () => copyText(context, text),
        child: Container(
          constraints: BoxConstraints(
            maxWidth: MediaQuery.sizeOf(context).width * 0.82,
          ),
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
          decoration: BoxDecoration(
            color: c.accent.withValues(alpha: .2),
            borderRadius: const BorderRadius.only(
              topLeft: r,
              topRight: r,
              bottomLeft: r,
              bottomRight: Radius.circular(4),
            ),
          ),
          child: Text(text, style: TextStyle(color: c.text, height: 1.4)),
        ),
      ),
    );
  }
}

/// What the agent said: full width, Markdown, no bubble.
class AssistantText extends StatelessWidget {
  const AssistantText(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) => GestureDetector(
    behavior: HitTestBehavior.translucent,
    onLongPress: () => copyText(context, text),
    child: SizedBox(width: double.infinity, child: AtlasMarkdown(text)),
  );
}

/// The agent's reasoning: one muted line, expands to the text.
class ThinkingRow extends StatefulWidget {
  const ThinkingRow(this.text, {super.key});
  final String text;

  @override
  State<ThinkingRow> createState() => _ThinkingRowState();
}

class _ThinkingRowState extends State<ThinkingRow> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        InkWell(
          borderRadius: BorderRadius.circular(Space.radiusSm),
          onTap: () => setState(() => _open = !_open),
          onLongPress: () => copyText(context, widget.text),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.psychology_outlined, size: 16, color: c.faint),
                const SizedBox(width: 6),
                Text(
                  'Thought',
                  style: TextStyle(
                    color: c.faint,
                    fontSize: 12.5,
                    fontStyle: FontStyle.italic,
                  ),
                ),
                Icon(
                  _open ? Icons.expand_less : Icons.expand_more,
                  size: 16,
                  color: c.faint,
                ),
              ],
            ),
          ),
        ),
        if (_open)
          Container(
            margin: const EdgeInsets.only(left: 7, top: 2, bottom: 4),
            padding: const EdgeInsets.only(left: 12),
            decoration: BoxDecoration(
              border: Border(left: BorderSide(color: c.border, width: 2)),
            ),
            child: Text(
              widget.text,
              style: TextStyle(color: c.faint, fontSize: 13, height: 1.45),
            ),
          ),
      ],
    );
  }
}

IconData toolIcon(String name) => switch (name) {
  'bash' => Icons.terminal,
  'read' => Icons.description_outlined,
  'edit' || 'write' || 'ast_edit' => Icons.edit_outlined,
  'grep' || 'search' || 'find' || 'glob' => Icons.search,
  'web_search' || 'fetch' => Icons.public,
  'task' => Icons.hub_outlined,
  'eval' || 'python' || 'js' => Icons.code,
  'todo_write' => Icons.checklist,
  _ => Icons.build_outlined,
};

class _StatusMark extends StatelessWidget {
  const _StatusMark(this.status);
  final String status;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return switch (status) {
      'running' => SizedBox.square(
        dimension: 14,
        child: CircularProgressIndicator(strokeWidth: 2, color: c.accent),
      ),
      'error' => Icon(Icons.close_rounded, size: 16, color: c.error),
      _ => Icon(Icons.check_rounded, size: 16, color: c.success),
    };
  }
}

/// A run of consecutive tool calls as one compact card: `7 steps · bash, read, edit`.
class ToolGroupCard extends StatefulWidget {
  const ToolGroupCard(this.group, {super.key});
  final ToolGroup group;

  @override
  State<ToolGroupCard> createState() => _ToolGroupCardState();
}

class _ToolGroupCardState extends State<ToolGroupCard> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final g = widget.group;
    final n = g.calls.length;
    final names = g.names;
    final shown = names.length > 4
        ? '${names.take(4).join(', ')}, +${names.length - 4}'
        : names.join(', ');
    // A running or failed step is worth seeing without a tap.
    final open = _open;
    return Container(
      decoration: BoxDecoration(
        color: c.base,
        border: Border.all(
          color: g.errors > 0 ? c.error.withValues(alpha: .4) : c.borderSubtle,
        ),
        borderRadius: BorderRadius.circular(Space.radius),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          InkWell(
            onTap: () => setState(() => _open = !_open),
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: Space.md,
                vertical: Space.sm + 2,
              ),
              child: Row(
                children: [
                  if (g.running)
                    SizedBox.square(
                      dimension: 14,
                      child: CircularProgressIndicator(
                        strokeWidth: 2,
                        color: c.accent,
                      ),
                    )
                  else
                    Icon(
                      g.errors > 0
                          ? Icons.error_outline_rounded
                          : Icons.bolt_rounded,
                      size: 16,
                      color: g.errors > 0 ? c.error : c.muted,
                    ),
                  const SizedBox(width: Space.sm),
                  Expanded(
                    child: Text.rich(
                      TextSpan(
                        children: [
                          TextSpan(
                            text: '$n ${n == 1 ? 'step' : 'steps'}',
                            style: TextStyle(
                              color: c.text,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          TextSpan(
                            text: ' · $shown',
                            style: TextStyle(color: c.muted),
                          ),
                        ],
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(fontSize: 13),
                    ),
                  ),
                  if (g.errors > 0) ...[
                    Pill('${g.errors} failed', color: c.error),
                    const SizedBox(width: 4),
                  ],
                  Icon(
                    open ? Icons.expand_less : Icons.expand_more,
                    size: 18,
                    color: c.faint,
                  ),
                ],
              ),
            ),
          ),
          if (open)
            for (final call in g.calls) ToolRow(call),
        ],
      ),
    );
  }
}

/// One tool call: icon, what it was for, status; taps open its output in mono.
class ToolRow extends StatefulWidget {
  const ToolRow(this.call, {super.key});
  final ToolItem call;

  @override
  State<ToolRow> createState() => _ToolRowState();
}

class _ToolRowState extends State<ToolRow> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final call = widget.call;
    final out = call.output?.trim() ?? '';
    final detail = call.intent != null ? call.detail?.trim() : null;
    final hasMore = out.isNotEmpty || (detail != null && detail.isNotEmpty);
    return Container(
      decoration: BoxDecoration(
        border: Border(top: BorderSide(color: c.borderSubtle)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          InkWell(
            onTap: hasMore ? () => setState(() => _open = !_open) : null,
            onLongPress: () => copyText(
              context,
              [call.summary, ?call.detail, ?call.output].join('\n'),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: Space.md,
                vertical: Space.sm,
              ),
              child: Row(
                children: [
                  Icon(toolIcon(call.name), size: 16, color: c.muted),
                  const SizedBox(width: Space.sm),
                  Expanded(
                    child: Text(
                      call.summary,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 13,
                        color: c.text,
                        fontFamily: call.intent == null ? monoFamily : null,
                        fontFamilyFallback: call.intent == null
                            ? monoFallback
                            : null,
                      ),
                    ),
                  ),
                  const SizedBox(width: Space.sm),
                  _StatusMark(call.status),
                ],
              ),
            ),
          ),
          if (_open)
            Container(
              margin: const EdgeInsets.fromLTRB(
                Space.md,
                0,
                Space.md,
                Space.sm,
              ),
              padding: const EdgeInsets.all(Space.sm),
              constraints: const BoxConstraints(maxHeight: 220),
              decoration: BoxDecoration(
                color: c.ground,
                borderRadius: BorderRadius.circular(Space.radiusSm),
              ),
              child: SingleChildScrollView(
                child: SelectableText(
                  [
                    if (detail != null && detail.isNotEmpty) '\$ $detail',
                    if (out.isNotEmpty) out,
                  ].join('\n\n'),
                  style: TextStyle(
                    fontFamily: monoFamily,
                    fontFamilyFallback: monoFallback,
                    fontSize: 12,
                    height: 1.4,
                    color: call.status == 'error' ? c.error : c.muted,
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// A `task` call: the subagents it started, each with its live state and report preview.
class SubagentsCard extends StatelessWidget {
  const SubagentsCard({
    super.key,
    required this.call,
    required this.history,
    required this.onOpen,
  });

  final ToolItem call;
  final AgentHistory history;
  final void Function(String name) onOpen;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final n = call.agents.length;
    return Container(
      decoration: BoxDecoration(
        color: c.base,
        border: Border.all(color: c.borderSubtle),
        borderRadius: BorderRadius.circular(Space.radius),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
              Space.md,
              Space.sm + 2,
              Space.md,
              Space.sm,
            ),
            child: Row(
              children: [
                Icon(Icons.hub_outlined, size: 16, color: c.muted),
                const SizedBox(width: Space.sm),
                Text(
                  n == 1 ? '1 subagent' : '$n subagents',
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: c.text,
                  ),
                ),
              ],
            ),
          ),
          for (final a in call.agents)
            _SubagentRow(
              spawned: a,
              info: history.subagent(a.name),
              // Without live info yet the call's own status says if it is still going.
              callRunning: call.status == 'running',
              onTap: () => onOpen(a.name),
            ),
        ],
      ),
    );
  }
}

class _SubagentRow extends StatelessWidget {
  const _SubagentRow({
    required this.spawned,
    required this.info,
    required this.callRunning,
    required this.onTap,
  });

  final SpawnedAgent spawned;
  final SubagentInfo? info;
  final bool callRunning;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final running = info?.running ?? callRunning;
    final report = info?.report?.trim().replaceAll(RegExp(r'\s+'), ' ');
    return InkWell(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(
          horizontal: Space.md,
          vertical: Space.sm + 2,
        ),
        decoration: BoxDecoration(
          border: Border(top: BorderSide(color: c.borderSubtle)),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 1),
              child: StatusDot(
                color: running ? c.accent : c.success,
                pulse: running,
                size: 7,
              ),
            ),
            const SizedBox(width: Space.sm),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Flexible(
                        child: Text(
                          spawned.name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            fontWeight: FontWeight.w600,
                            fontSize: 13.5,
                          ),
                        ),
                      ),
                      if (spawned.agent != null) ...[
                        const SizedBox(width: Space.sm),
                        Pill(spawned.agent!, color: c.muted),
                      ],
                      const SizedBox(width: Space.sm),
                      Text(
                        running ? 'running' : 'done',
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                          color: running ? c.accent : c.success,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 2),
                  Text(
                    spawned.task,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(fontSize: 12.5, color: c.muted),
                  ),
                  if (report != null && report.isNotEmpty) ...[
                    const SizedBox(height: 4),
                    Text(
                      report,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 12.5,
                        color: c.text,
                        height: 1.35,
                      ),
                    ),
                  ],
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Icon(Icons.chevron_right, size: 18, color: c.faint),
            ),
          ],
        ),
      ),
    );
  }
}

/// One timeline block, in order.
Widget timelineBlock(
  BuildContext context,
  TimelineBlock b, {
  required AgentHistory history,
  required void Function(String name) onOpenSubagent,
}) => switch (b) {
  ItemBlock(item: UserItem(:final text)) => UserBubble(text),
  ItemBlock(item: AssistantItem(:final text)) => AssistantText(text),
  ItemBlock(item: ThinkingItem(:final text)) => ThinkingRow(text),
  ItemBlock() => const SizedBox.shrink(),
  ToolGroup() => ToolGroupCard(b),
  SubagentsBlock(:final call) => SubagentsCard(
    call: call,
    history: history,
    onOpen: onOpenSubagent,
  ),
};
