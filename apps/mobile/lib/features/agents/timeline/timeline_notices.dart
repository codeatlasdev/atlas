import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/timeline_items.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/timeline/copy_text.dart';

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
                  style: Theme.of(context).textTheme.bodyMedium!,
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
                  style: Theme.of(context).textTheme.bodySmall!
                      .copyWith(color: c.muted, fontStyle: FontStyle.italic),
                ),
              ),
              Expanded(child: Divider(color: c.borderSubtle)),
            ],
          ),
        ),
        if (_open)
          Padding(
            padding: const EdgeInsets.only(top: Space.xs),
            child: Text(
              widget.text,
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: c.muted, height: 1.4),
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
                  const SizedBox(width: Space.sm),
                  Expanded(
                    child: Text(
                      fromParent
                          ? 'Message from ${item.sender} (parent)'
                          : 'Message from ${item.sender}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.bodySmall!
                          .copyWith(fontWeight: FontWeight.w700, color: c.info),
                    ),
                  ),
                  if (onOpen != null)
                    Icon(Icons.chevron_right, size: 16, color: c.faint),
                ],
              ),
              const SizedBox(height: Space.xs),
              _NoteText(item.text),
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
            const SizedBox(width: Space.sm),
            Expanded(
              child: Text(
                _open ? widget.text : first,
                maxLines: _open ? null : 1,
                overflow: _open ? null : TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodySmall!.copyWith(
                  color: c.muted,
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
              const SizedBox(width: Space.sm),
              Text(
                'Advisor · ${item.severity}',
                style: Theme.of(context).textTheme.bodySmall!
                    .copyWith(fontWeight: FontWeight.w700, color: color),
              ),
            ],
          ),
          const SizedBox(height: Space.xs),
          _NoteText(item.text),
        ],
      ),
    );
  }
}

/// The text of a card that carries a note: IRC messages and advisor findings.
class _NoteText extends StatelessWidget {
  const _NoteText(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Text(
    text,
    style: Theme.of(context).textTheme.bodyMedium!
        .copyWith(color: AtlasColors.of(context).text, height: 1.4),
  );
}
