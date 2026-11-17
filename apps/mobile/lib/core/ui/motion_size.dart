import 'package:material_ui/material_ui.dart';

import 'atlas_theme.dart';

/// Content that grows or shrinks in place (a status line arriving, a prompt above a composer): it eases to its new
/// size with motion on, and is simply at its size under Reduce motion.
///
/// The one way to animate a size in the app (a test forbids `AnimatedSize` elsewhere): an `AnimatedSize` given
/// `Duration.zero` for Reduce motion re-dirties itself while it lays out, and one with a fixed duration ignores
/// Reduce motion.
class MotionSize extends StatelessWidget {
  const MotionSize({
    super.key,
    required this.child,
    this.alignment = Alignment.topCenter,
    this.duration,
  });

  final Widget child;

  /// The edge that stays put: `topCenter` grows downward (nothing above moves), `bottomCenter` grows upward
  /// (for things above a composer).
  final AlignmentGeometry alignment;

  /// Defaults to [Motion.normal].
  final Duration? duration;

  @override
  Widget build(BuildContext context) {
    if (Motion.reduced(context)) return child;
    return AnimatedSize(
      duration: duration ?? Motion.normal,
      // Eases in as well as out: a size change must not jump in its first frame.
      curve: Curves.easeInOut,
      alignment: alignment,
      child: child,
    );
  }
}
