import 'package:material_ui/material_ui.dart';

import 'atlas_theme.dart';

/// A list row in the chat-list shape (crewmates, projects, activity): the [leading] avatar, then the
/// [lines] stacked, and a hairline under them that starts after the avatar and is left off the [last] row.
/// Tappable with [onTap]; with a [semanticsLabel] the row is one button named by it (its parts are not
/// read one by one).
class InsetRow extends StatelessWidget {
  const InsetRow({
    super.key,
    required this.leading,
    required this.lines,
    required this.last,
    this.onTap,
    this.semanticsLabel,
  });

  final Widget leading;
  final List<Widget> lines;
  final bool last;
  final VoidCallback? onTap;
  final String? semanticsLabel;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final row = Material(
      type: MaterialType.transparency,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.only(left: Space.lg),
          child: Row(
            children: [
              leading,
              const SizedBox(width: Space.md),
              Expanded(
                child: Container(
                  padding: const EdgeInsets.fromLTRB(
                    0,
                    Space.md,
                    Space.lg,
                    Space.md,
                  ),
                  decoration: BoxDecoration(
                    border: last
                        ? null
                        : Border(bottom: BorderSide(color: c.borderSubtle)),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: lines,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
    if (semanticsLabel == null) return row;
    return Semantics(
      button: true,
      label: semanticsLabel,
      excludeSemantics: true,
      onTap: onTap,
      child: row,
    );
  }
}
