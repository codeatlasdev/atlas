import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/markdown/atlas_markdown.dart';
import 'package:atlas_mobile/features/agents/timeline/copy_text.dart';

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
          padding: const EdgeInsets.symmetric(
            horizontal: Space.md,
            vertical: Space.md,
          ),
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
          // The label is small; the touch area is not.
          child: MinTapTarget(
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.psychology_outlined, size: 16, color: c.faint),
                const SizedBox(width: Space.sm),
                Text(
                  'Thought',
                  style: Theme.of(context).textTheme.bodySmall!
                      .copyWith(color: c.muted, fontStyle: FontStyle.italic),
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
            margin: const EdgeInsets.only(
              left: Space.sm,
              top: 2,
              bottom: Space.xs,
            ),
            padding: const EdgeInsets.only(left: Space.md),
            decoration: BoxDecoration(
              border: Border(left: BorderSide(color: c.border, width: 2)),
            ),
            child: Text(
              widget.text,
              style: Theme.of(context).textTheme.bodyMedium!
                  .copyWith(color: c.muted, height: 1.45),
            ),
          ),
      ],
    );
  }
}
