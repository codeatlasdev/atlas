import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/timeline_blocks.dart';
import 'package:atlas_mobile/core/protocol/timeline_items.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/timeline/copy_text.dart';

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
                      style: Theme.of(context).textTheme.bodyMedium!,
                    ),
                  ),
                  if (g.errors > 0) ...[
                    AtlasChip('${g.errors} failed', tone: c.error),
                    const SizedBox(width: Space.xs),
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
                      style: Theme.of(context).textTheme.bodyMedium!.copyWith(
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
                  style: Theme.of(context).textTheme.bodySmall!.copyWith(
                    fontFamily: monoFamily,
                    fontFamilyFallback: monoFallback,
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
