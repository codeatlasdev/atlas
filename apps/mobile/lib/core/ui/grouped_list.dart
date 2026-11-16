import 'package:material_ui/material_ui.dart';

import 'atlas_theme.dart';
import 'components.dart';

/// A titled group of rows on one card, rows split by hairlines inset past their icon: the one shape every
/// settings-like list in the app uses (Settings, a computer's details), so they share spacing and rhythm.
class RowGroup extends StatelessWidget {
  const RowGroup({super.key, this.title, this.footer, required this.children});

  final String? title;

  /// A short line under the card: what the group is for, or a hint.
  final String? footer;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.xl),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (title != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(
                Space.xs,
                0,
                Space.xs,
                Space.sm,
              ),
              child: Semantics(
                header: true,
                child: Text(
                  title!,
                  style: t.titleSmall!.copyWith(color: c.muted),
                ),
              ),
            ),
          DecoratedBox(
            decoration: BoxDecoration(
              color: c.base,
              borderRadius: BorderRadius.circular(Space.radius),
              border: Border.all(color: c.borderSubtle),
            ),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(Space.radius),
              child: Column(
                children: [
                  for (var i = 0; i < children.length; i++) ...[
                    if (i > 0)
                      Divider(
                        height: 1,
                        thickness: 1,
                        indent: GroupRow.textInset,
                        color: c.borderSubtle,
                      ),
                    children[i],
                  ],
                ],
              ),
            ),
          ),
          if (footer != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(
                Space.xs,
                Space.sm,
                Space.xs,
                0,
              ),
              child: Text(
                footer!,
                style: t.bodySmall!.copyWith(color: c.muted),
              ),
            ),
        ],
      ),
    );
  }
}

/// What a [GroupRow] shows at its end when it has no [GroupRow.trailing].
enum RowKind {
  /// Opens a screen in the app: a chevron.
  navigate,

  /// Opens a page in the browser: an arrow out of the box.
  external,

  /// Shows a value: nothing.
  info,
}

/// One row of a [RowGroup]: a tinted icon, a title, an optional second line, and what it does at the end.
/// At least 56 dp tall (a two-line row 64+), grows with the text size, never clips.
class GroupRow extends StatelessWidget {
  const GroupRow({
    super.key,
    required this.icon,
    required this.title,
    this.subtitle,
    this.status,
    this.trailing,
    this.onTap,
    this.kind = RowKind.navigate,
    this.tint,
    this.destructive = false,
  });

  final IconData icon;
  final String title;
  final String? subtitle;

  /// A state dot before the [subtitle] (connected, connecting, offline).
  final Color? status;

  /// A control at the end (a menu, a switch). It brings its own 48 dp touch area, so the row's end padding
  /// shrinks to keep its icon where a chevron would be.
  final Widget? trailing;
  final VoidCallback? onTap;
  final RowKind kind;

  /// The icon's colour; the accent by default.
  final Color? tint;
  final bool destructive;

  static const _iconBox = 32.0;

  /// Where the text starts: the divider between rows begins here.
  static const textInset = Space.lg + _iconBox + Space.md;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
    final tone = destructive ? c.error : (tint ?? c.accent);
    final end =
        trailing ??
        switch (kind) {
          RowKind.navigate when onTap != null => Icon(
            Icons.chevron_right_rounded,
            color: c.faint,
          ),
          RowKind.external => Icon(
            Icons.arrow_outward_rounded,
            size: 18,
            color: c.faint,
          ),
          _ => null,
        };
    return Semantics(
      button: onTap != null,
      link: kind == RowKind.external,
      child: InkWell(
        onTap: onTap,
        child: ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 56),
          child: Padding(
            padding: EdgeInsets.fromLTRB(
              Space.lg,
              Space.md,
              trailing == null ? Space.lg : Space.xs,
              Space.md,
            ),
            child: Row(
              children: [
                Container(
                  width: _iconBox,
                  height: _iconBox,
                  decoration: BoxDecoration(
                    color: tone.withValues(alpha: .14),
                    borderRadius: BorderRadius.circular(Space.radiusSm),
                  ),
                  child: Icon(icon, size: 18, color: tone),
                ),
                const SizedBox(width: Space.md),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        title,
                        style: t.bodyLarge!.copyWith(
                          color: destructive ? c.error : c.text,
                        ),
                      ),
                      if (subtitle != null) ...[
                        const SizedBox(height: 2),
                        Row(
                          children: [
                            if (status != null) ...[
                              StatusDot(color: status!, size: 7),
                              const SizedBox(width: 6),
                            ],
                            Flexible(
                              // A live status line ("Connected · Atlas 1.0.0") stays one line: its parts arrive
                              // one by one, and a wrap would make the row grow under the reader's eyes.
                              child: Text(
                                subtitle!,
                                maxLines: status == null ? null : 1,
                                overflow: status == null
                                    ? null
                                    : TextOverflow.ellipsis,
                                style: t.bodySmall!.copyWith(color: c.muted),
                              ),
                            ),
                          ],
                        ),
                      ],
                    ],
                  ),
                ),
                if (end != null) ...[const SizedBox(width: Space.sm), end],
              ],
            ),
          ),
        ),
      ),
    );
  }
}
