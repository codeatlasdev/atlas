/// The mode of the crewmate you are talking to, in the chat header: the glyph and the name in a frosted pill, amber
/// for Bypass only. It opens the mode sheet; for an engine with no approval channel it opens the same sheet with
/// the choice fixed and the reason.
library;

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/features/crew/chat/glass.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_text.dart';

class ModeButton extends StatelessWidget {
  const ModeButton({super.key, required this.crewmate, required this.onTap});

  final CrewmateEntry crewmate;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final mode = crewmate.mode;
    final fixed = !crewmate.approvalSupported;
    final name = mode.label(l);
    final color = mode.color(c);
    return Semantics(
      button: true,
      label: fixed
          ? l.mobile_crew_mode_locked_semantics(name)
          : l.mobile_crew_mode_semantics(name),
      hint: fixed ? null : l.mobile_crew_mode_change,
      excludeSemantics: true,
      onTap: onTap,
      child: Pressable(
        onTap: onTap,
        // The pill is drawn at its own height; the hit area is the 48 dp a thumb needs.
        child: ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 48),
          child: Center(
            widthFactor: 1,
            child: GlassPill(
              padding: const EdgeInsets.symmetric(
                horizontal: Space.md,
                vertical: Space.sm,
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(mode.icon, size: 16, color: color),
                  const SizedBox(width: Space.xs),
                  Flexible(
                    child: Text(
                      name,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.bodyMedium!.copyWith(
                        color: color == c.muted ? c.text : color,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  if (!fixed) ...[
                    const SizedBox(width: Space.xs),
                    Icon(
                      Icons.keyboard_arrow_down_rounded,
                      size: 18,
                      color: c.muted,
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
