/// The frosted pieces floating over the chat thread: the pill and the round header button.
library;

import 'dart:ui' show ImageFilter;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';

/// A round, softly filled header button (back), like the reference's.
class GlassRoundButton extends StatelessWidget {
  const GlassRoundButton({
    super.key,
    required this.icon,
    required this.label,
    required this.onTap,
  });
  final IconData icon;
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Semantics(
    button: true,
    label: label,
    child: Pressable(
      onTap: onTap,
      child: GlassPill(
        circle: true,
        child: SizedBox.square(
          dimension: 44,
          child: Icon(icon, size: 18, color: AtlasColors.of(context).text),
        ),
      ),
    ),
  );
}

/// A floating piece over the thread: frosted, hairline edge, soft shadow.
class GlassPill extends StatelessWidget {
  const GlassPill({
    super.key,
    required this.child,
    this.circle = false,
    this.padding,
  });
  final Widget child;
  final bool circle;
  final EdgeInsets? padding;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final shape = circle ? const CircleBorder() : const StadiumBorder();
    return ClipPath(
      clipper: ShapeBorderClipper(shape: shape),
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 18, sigmaY: 18),
        child: Container(
          padding: padding,
          decoration: ShapeDecoration(
            color: c.overlay.withValues(alpha: .82),
            shape: circle
                ? CircleBorder(side: BorderSide(color: c.border))
                : StadiumBorder(side: BorderSide(color: c.border)),
          ),
          child: child,
        ),
      ),
    );
  }
}
