import 'package:flutter/material.dart';

import 'atlas_theme.dart';

/// The app's ground under every route: `MaterialApp.builder` for the app and every test harness. Scaffolds are
/// transparent (screens draw [AtlasBackdrop]), so without this a screen or sheet that has no backdrop of its
/// own would show the window behind it (white in tests, black on a device) instead of the app's colour.
Widget atlasGround(BuildContext context, Widget? child) =>
    ColoredBox(color: AtlasColors.of(context).ground, child: child);

/// The one background every screen sits on: the app ground and a soft glow from the top, the app icon's blue
/// by default or a screen's own colour ([tint], a project's). Every tab and pushed screen uses it, so moving
/// between them never changes the light in the room.
class AtlasBackdrop extends StatelessWidget {
  const AtlasBackdrop({super.key, required this.child, this.tint});

  final Widget child;

  /// The glow's colour; null is the app's blue.
  final Color? tint;

  /// The app icon's blue, the default glow.
  static const brand = Color(0xFF3D5BFF);

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final glow = tint ?? brand;
    return ColoredBox(
      color: c.ground,
      child: Stack(
        children: [
          Positioned(
            top: -220,
            left: -120,
            right: -120,
            height: 520,
            child: IgnorePointer(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  gradient: RadialGradient(
                    radius: .62,
                    colors: [
                      glow.withValues(alpha: .20),
                      glow.withValues(alpha: .07),
                      glow.withValues(alpha: 0),
                    ],
                    stops: const [0, .55, 1],
                  ),
                ),
              ),
            ),
          ),
          Positioned.fill(child: child),
        ],
      ),
    );
  }
}
