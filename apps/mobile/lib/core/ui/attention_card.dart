import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// Which [AtlasColors] member tints an [AttentionCard].
enum AttentionTone { warning, danger, info, accent }

/// How loud the tint is. The numbers are the ones the screens had before they shared this card.
enum AttentionStrength {
  /// Crew cards (paused, captain's call, a task waiting on you, a question): fill .08, border .35.
  quiet(.08, .35),

  /// The chat bubble that carries Deny / Allow: fill .10, border .40.
  firm(.10, .40),

  /// An agent's approval banner, the thing that blocks the agent: fill .12, border .60.
  loud(.12, .60);

  const AttentionStrength(this.fill, this.border);
  final double fill;
  final double border;
}

/// The chat bubble's shape: a square tail corner at the top left, 18 elsewhere.
const _bubbleRadius = BorderRadius.only(
  topLeft: Radius.circular(4),
  topRight: Radius.circular(18),
  bottomLeft: Radius.circular(18),
  bottomRight: Radius.circular(18),
);

/// The one "this needs you" surface: a tone-tinted card with a tone border. Pass either a [child] (the
/// caller lays it out) or [title] / [body] / [actions] (an optional [icon] in front); never both.
///
/// [bubble] makes it a chat message: the tail corner, 12 px padding and at most 80% of the screen wide.
class AttentionCard extends StatelessWidget {
  const AttentionCard({
    super.key,
    this.tone = AttentionTone.warning,
    this.strength = AttentionStrength.quiet,
    this.bubble = false,
    this.radius,
    this.padding,
    this.margin,
    this.icon,
    this.title,
    this.body,
    this.actions = const [],
    this.child,
  }) : assert(
         child == null || (title == null && body == null && icon == null),
         'a child lays itself out: title, body and icon are for the built-in layout',
       );

  final AttentionTone tone;
  final AttentionStrength strength;
  final bool bubble;

  /// Overrides the corners (default 14, or the bubble's).
  final BorderRadius? radius;

  /// Overrides the padding (default 14, or 12 for a bubble).
  final EdgeInsetsGeometry? padding;
  final EdgeInsetsGeometry? margin;
  final IconData? icon;
  final String? title;
  final String? body;
  final List<Widget> actions;
  final Widget? child;

  Color _color(AtlasColors c) => switch (tone) {
    AttentionTone.warning => c.warning,
    AttentionTone.danger => c.error,
    AttentionTone.info => c.info,
    AttentionTone.accent => c.accent,
  };

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final color = _color(c);
    // Past this text scale an icon, two lines of text and a button no longer fit one row: the
    // actions go under the text instead of squeezing it to nothing.
    final stacked =
        actions.isNotEmpty && MediaQuery.textScalerOf(context).scale(14) > 18;
    final text = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (title != null)
          Text(
            title!,
            style: Theme.of(context).textTheme.bodyLarge!
                .copyWith(fontWeight: FontWeight.w700, color: c.text),
          ),
        if (body != null) ...[
          if (title != null) const SizedBox(height: 2),
          Text(
            body!,
            style: Theme.of(context).textTheme.bodySmall!
                .copyWith(color: c.muted),
          ),
        ],
      ],
    );
    final content =
        child ??
        (stacked
            ? Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      if (icon != null) ...[
                        Icon(icon, color: color, size: 22),
                        const SizedBox(width: 10),
                      ],
                      Expanded(child: text),
                    ],
                  ),
                  const SizedBox(height: 8),
                  Wrap(spacing: 10, runSpacing: 4, children: actions),
                ],
              )
            : Row(
                children: [
                  if (icon != null) ...[
                    Icon(icon, color: color, size: 22),
                    const SizedBox(width: 10),
                  ],
                  Expanded(child: text),
                  if (actions.isNotEmpty) ...[
                    const SizedBox(width: 10),
                    ...actions,
                  ],
                ],
              ));
    return Semantics(
      container: true,
      liveRegion: true,
      child: Container(
        margin: margin,
        padding: padding ?? EdgeInsets.all(bubble ? 12 : 14),
        constraints: bubble
            ? BoxConstraints(maxWidth: MediaQuery.sizeOf(context).width * 0.8)
            : null,
        decoration: BoxDecoration(
          color: color.withValues(alpha: strength.fill),
          border: Border.all(color: color.withValues(alpha: strength.border)),
          borderRadius:
              radius ?? (bubble ? _bubbleRadius : BorderRadius.circular(14)),
        ),
        child: content,
      ),
    );
  }
}
