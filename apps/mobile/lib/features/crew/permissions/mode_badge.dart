/// The mode of a crewmate, drawn small wherever it appears (crew list row, chat header, answer cards): the glyph and
/// the name, amber for Bypass only. A screen reader hears "Permission mode: Ask" (and that it is fixed, for an
/// engine with no approval channel).
library;

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/permission_mode.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_text.dart';

class ModeBadge extends StatelessWidget {
  const ModeBadge({super.key, required this.mode, this.fixed = false});

  final PermissionMode mode;

  /// The mode cannot be changed (the engine has no approval channel).
  final bool fixed;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final color = mode.color(c);
    final name = mode.label(l);
    return Semantics(
      label: fixed
          ? l.mobile_crew_mode_locked_semantics(name)
          : l.mobile_crew_mode_semantics(name),
      child: ExcludeSemantics(
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(mode.icon, size: 14, color: color),
            const SizedBox(width: Space.xs),
            Flexible(
              child: Text(
                name,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodySmall!.copyWith(
                  color: color,
                  fontWeight: mode == PermissionMode.bypass
                      ? FontWeight.w600
                      : FontWeight.w500,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
