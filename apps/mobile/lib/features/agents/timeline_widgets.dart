import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/markdown/atlas_markdown.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';

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

/// The summary at the top of a turn: files touched, commands, subagents, time.
class RecapCard extends StatelessWidget {
  const RecapCard(this.recap, {super.key});
  final Recap recap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final r = recap;
    final files = r.files.length;
    final parts = <(IconData, String, Color)>[
      if (files > 0)
        (
          Icons.edit_note_rounded,
          '$files ${files == 1 ? 'file' : 'files'} changed',
          c.text,
        ),
      if (r.commands > 0)
        (
          Icons.terminal,
          '${r.commands} ${r.commands == 1 ? 'command' : 'commands'}',
          c.text,
        ),
      if (r.finished > 0)
        (
          Icons.hub_outlined,
          '${r.finished} ${r.finished == 1 ? 'subagent' : 'subagents'} done',
          c.success,
        ),
      if (r.failed > 0)
        (Icons.error_outline_rounded, '${r.failed} failed', c.error),
      if (r.duration != null && r.duration!.inSeconds > 0)
        (
          Icons.timer_outlined,
          durationText(r.duration!.inMilliseconds),
          c.muted,
        ),
    ];
    return Container(
      padding: const EdgeInsets.all(Space.md),
      decoration: BoxDecoration(
        color: c.accent.withValues(alpha: .07),
        border: Border.all(color: c.accent.withValues(alpha: .25)),
        borderRadius: BorderRadius.circular(Space.radius),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.auto_awesome, size: 14, color: c.accent),
              const SizedBox(width: 6),
              Text(
                'Recap',
                style: TextStyle(
                  fontSize: 12.5,
                  fontWeight: FontWeight.w700,
                  color: c.accent,
                ),
              ),
            ],
          ),
          const SizedBox(height: Space.sm),
          Wrap(
            spacing: Space.md,
            runSpacing: 4,
            children: [
              for (final (icon, label, color) in parts)
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(icon, size: 15, color: color),
                    const SizedBox(width: 4),
                    Text(label, style: TextStyle(fontSize: 13, color: color)),
                  ],
                ),
            ],
          ),
          if (files > 0) ...[
            const SizedBox(height: Space.sm),
            for (final f in r.files.take(4))
              Text(
                f,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontFamily: monoFamily,
                  fontFamilyFallback: monoFallback,
                  fontSize: 12,
                  color: c.muted,
                ),
              ),
            if (files > 4)
              Text(
                '+${files - 4} more',
                style: TextStyle(fontSize: 12, color: c.faint),
              ),
          ],
        ],
      ),
    );
  }
}

/// "Scout finished · 41s": the subagent's result landed; tap opens its transcript.
class ResultCard extends StatelessWidget {
  const ResultCard(this.item, {super.key, required this.onOpen});
  final ResultItem item;
  final void Function(String id) onOpen;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (icon, color, verb) = switch (item.status) {
      'completed' => (Icons.check_circle_rounded, c.success, 'finished'),
      'failed' => (Icons.cancel_rounded, c.error, 'failed'),
      _ => (Icons.stop_circle_rounded, c.warning, 'stopped'),
    };
    return Material(
      color: color.withValues(alpha: .08),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Space.radius),
        side: BorderSide(color: color.withValues(alpha: .3)),
      ),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: () => onOpen(item.id),
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: Space.md,
            vertical: Space.sm + 2,
          ),
          child: Row(
            children: [
              Icon(icon, size: 18, color: color),
              const SizedBox(width: Space.sm),
              Expanded(
                child: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(
                        text: item.id,
                        style: TextStyle(
                          fontWeight: FontWeight.w600,
                          color: c.text,
                        ),
                      ),
                      TextSpan(
                        text: ' $verb',
                        style: TextStyle(color: color),
                      ),
                      if (item.duration != null)
                        TextSpan(
                          text: ' · ${item.duration}',
                          style: TextStyle(color: c.muted),
                        ),
                    ],
                  ),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontSize: 13),
                ),
              ),
              Icon(Icons.chevron_right, size: 18, color: c.faint),
            ],
          ),
        ),
      ),
    );
  }
}

/// The context was compacted here: a hairline with the summary on tap.
class CompactionDivider extends StatefulWidget {
  const CompactionDivider(this.text, {super.key});
  final String text;

  @override
  State<CompactionDivider> createState() => _CompactionDividerState();
}

class _CompactionDividerState extends State<CompactionDivider> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      children: [
        InkWell(
          onTap: widget.text.isEmpty
              ? null
              : () => setState(() => _open = !_open),
          child: Row(
            children: [
              Expanded(child: Divider(color: c.borderSubtle)),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: Space.sm),
                child: Text(
                  'context compacted',
                  style: TextStyle(
                    fontSize: 11.5,
                    color: c.faint,
                    fontStyle: FontStyle.italic,
                  ),
                ),
              ),
              Expanded(child: Divider(color: c.borderSubtle)),
            ],
          ),
        ),
        if (_open)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
              widget.text,
              style: TextStyle(fontSize: 12.5, color: c.faint, height: 1.4),
            ),
          ),
      ],
    );
  }
}

/// A message another agent sent this one (omp's IRC): a compact card with the sender, never
/// the user's bubble.
class IrcCard extends StatelessWidget {
  const IrcCard(this.item, {super.key, this.onOpen});
  final IrcItem item;

  /// Opens the sender's conversation (`''` = the main agent); null = not a link.
  final VoidCallback? onOpen;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final fromParent = item.from == 'parent';
    return Material(
      color: c.info.withValues(alpha: .08),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Space.radius),
        side: BorderSide(color: c.info.withValues(alpha: .3)),
      ),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onOpen,
        onLongPress: () => copyText(context, item.text),
        child: Padding(
          padding: const EdgeInsets.all(Space.md),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(Icons.forum_outlined, size: 15, color: c.info),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      fromParent
                          ? 'Message from ${item.sender} (parent)'
                          : 'Message from ${item.sender}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 12.5,
                        fontWeight: FontWeight.w700,
                        color: c.info,
                      ),
                    ),
                  ),
                  if (onOpen != null)
                    Icon(Icons.chevron_right, size: 16, color: c.faint),
                ],
              ),
              const SizedBox(height: 4),
              Text(
                item.text,
                style: TextStyle(color: c.text, height: 1.4, fontSize: 13.5),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// A notice the harness injected: one muted line, expands to the whole text.
class NoticeRow extends StatefulWidget {
  const NoticeRow(this.text, {super.key});
  final String text;

  @override
  State<NoticeRow> createState() => _NoticeRowState();
}

class _NoticeRowState extends State<NoticeRow> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final first = widget.text.split('\n').first;
    return InkWell(
      onTap: () => setState(() => _open = !_open),
      onLongPress: () => copyText(context, widget.text),
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(Icons.info_outline_rounded, size: 14, color: c.faint),
            const SizedBox(width: 6),
            Expanded(
              child: Text(
                _open ? widget.text : first,
                maxLines: _open ? null : 1,
                overflow: _open ? null : TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 12.5,
                  color: c.faint,
                  fontStyle: FontStyle.italic,
                  height: 1.35,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// omp's advisor note, coloured by severity: blocker red, concern amber, anything else grey.
class AdvisorCard extends StatelessWidget {
  const AdvisorCard(this.item, {super.key});
  final AdvisorItem item;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (color, icon) = switch (item.severity) {
      'blocker' => (c.error, Icons.report_rounded),
      'concern' => (c.warning, Icons.warning_amber_rounded),
      _ => (c.muted, Icons.lightbulb_outline_rounded),
    };
    return Container(
      decoration: BoxDecoration(
        color: color.withValues(alpha: .08),
        borderRadius: BorderRadius.circular(Space.radius),
        border: Border(left: BorderSide(color: color, width: 3)),
      ),
      padding: const EdgeInsets.all(Space.md),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(icon, size: 15, color: color),
              const SizedBox(width: 6),
              Text(
                'Advisor · ${item.severity}',
                style: TextStyle(
                  fontSize: 12.5,
                  fontWeight: FontWeight.w700,
                  color: color,
                ),
              ),
            ],
          ),
          const SizedBox(height: 4),
          Text(
            item.text,
            style: TextStyle(color: c.text, height: 1.4, fontSize: 13.5),
          ),
        ],
      ),
    );
  }
}

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
