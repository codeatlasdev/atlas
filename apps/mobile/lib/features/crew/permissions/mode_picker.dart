/// The choice of a permission mode: Ask, Accept edits and Bypass as three rows, each with its glyph, name and what
/// it lets run. Used by the chat's mode sheet, the profile, the recruit sheet and the crew settings' default.
///
/// Bypass is left out when the crew does not allow it (unless it is the current value, so a crewmate that is in it is
/// never shown as something else). A [reason] with no [onChanged] is the fixed control of an engine that cannot ask.
library;

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/permission_mode.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_text.dart';

class ModePicker extends StatelessWidget {
  const ModePicker({
    super.key,
    required this.value,
    required this.onChanged,
    this.allowBypass = true,
    this.reason,
  });

  final PermissionMode value;

  /// Null: the control is fixed.
  final ValueChanged<PermissionMode>? onChanged;
  final bool allowBypass;

  /// Why the control is fixed, shown under the rows.
  final String? reason;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final shown = [
      for (final m in PermissionMode.values)
        if (m != PermissionMode.bypass || allowBypass || value == m) m,
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final m in shown)
          _ModeRow(
            mode: m,
            selected: m == value,
            // The selected row of a fixed control stays readable; the others are dimmed.
            onTap: onChanged == null || m == value ? null : () => onChanged!(m),
            fixed: onChanged == null,
          ),
        if (reason != null)
          Padding(
            padding: const EdgeInsets.only(top: Space.sm),
            child: Text(
              reason!,
              style: Theme.of(context).textTheme.bodyMedium!
                  .copyWith(color: c.muted, height: 1.4),
            ),
          ),
      ],
    );
  }
}

class _ModeRow extends StatelessWidget {
  const _ModeRow({
    required this.mode,
    required this.selected,
    required this.onTap,
    required this.fixed,
  });

  final PermissionMode mode;
  final bool selected;
  final VoidCallback? onTap;
  final bool fixed;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final amber = mode == PermissionMode.bypass;
    final edge = selected ? (amber ? c.warning : c.accent) : c.borderSubtle;
    final name = mode.label(l);
    final detail = mode.detail(l);
    return Semantics(
      container: true,
      inMutuallyExclusiveGroup: true,
      checked: selected,
      enabled: !fixed,
      button: !fixed,
      label: '$name. $detail',
      onTap: onTap,
      child: ExcludeSemantics(
        child: Opacity(
          opacity: fixed && !selected ? .5 : 1,
          child: Padding(
            padding: const EdgeInsets.only(bottom: Space.sm),
            child: Material(
              color: selected
                  ? edge.withValues(alpha: .10)
                  : Colors.transparent,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(Space.radius),
                side: BorderSide(color: edge),
              ),
              child: InkWell(
                onTap: onTap,
                borderRadius: BorderRadius.circular(Space.radius),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(minHeight: 56),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: Space.md,
                      vertical: Space.sm,
                    ),
                    child: Row(
                      children: [
                        Icon(mode.icon, size: 20, color: mode.color(c)),
                        const SizedBox(width: Space.md),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                name,
                                style: Theme.of(context).textTheme.bodyLarge!
                                    .copyWith(
                                      fontWeight: FontWeight.w600,
                                      color: amber ? c.warning : c.text,
                                    ),
                              ),
                              Text(
                                detail,
                                style: Theme.of(context).textTheme.bodySmall!
                                    .copyWith(color: c.muted, height: 1.35),
                              ),
                            ],
                          ),
                        ),
                        const SizedBox(width: Space.sm),
                        Icon(
                          selected
                              ? Icons.radio_button_checked_rounded
                              : Icons.radio_button_unchecked_rounded,
                          size: 20,
                          color: selected ? edge : c.faint,
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
