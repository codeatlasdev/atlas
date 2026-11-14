/// The compact pill buttons of the crew cards (approval, task action): a warning-filled
/// primary and an outlined secondary, one shape so the cards read as one family.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// Tight pill: no minimum size, shrink-wrapped tap target, fully rounded.
ButtonStyle _pill({
  required EdgeInsets padding,
  Color? foreground,
  Color? background,
  BorderSide? side,
}) => ButtonStyle(
  backgroundColor: background == null
      ? null
      : WidgetStatePropertyAll(background),
  foregroundColor: foreground == null
      ? null
      : WidgetStatePropertyAll(foreground),
  side: side == null ? null : WidgetStatePropertyAll(side),
  padding: WidgetStatePropertyAll(padding),
  minimumSize: const WidgetStatePropertyAll(Size.zero),
  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
  shape: WidgetStatePropertyAll(
    RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
  ),
);

/// Primary action of a crew card: warning colour, label in the button's default size.
class WarnPillButton extends StatelessWidget {
  const WarnPillButton({
    super.key,
    required this.label,
    required this.onPressed,
    this.hPad = 14,
    this.vPad = 6,
  });

  final String label;
  final VoidCallback? onPressed;
  final double hPad;
  final double vPad;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return FilledButton(
      onPressed: onPressed,
      style: _pill(
        padding: EdgeInsets.symmetric(horizontal: hPad, vertical: vPad),
        background: c.warning,
        foreground: c.ground,
      ),
      child: Text(label),
    );
  }
}

/// Secondary action of a crew card. [danger] paints it as a refusal (Deny).
class OutlinePillButton extends StatelessWidget {
  const OutlinePillButton({
    super.key,
    required this.label,
    required this.onPressed,
    this.danger = false,
    this.fontSize,
    this.hPad = 14,
    this.vPad = 6,
  });

  final String label;
  final VoidCallback? onPressed;
  final bool danger;

  /// Null: the button's default label size.
  final double? fontSize;
  final double hPad;
  final double vPad;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return OutlinedButton(
      onPressed: onPressed,
      style: _pill(
        padding: EdgeInsets.symmetric(horizontal: hPad, vertical: vPad),
        foreground: danger ? c.error : null,
        side: danger ? BorderSide(color: c.error.withValues(alpha: .5)) : null,
      ),
      child: fontSize == null
          ? Text(label)
          : Text(label, style: TextStyle(fontSize: fontSize)),
    );
  }
}
