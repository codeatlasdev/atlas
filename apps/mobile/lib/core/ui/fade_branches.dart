import 'package:flutter/material.dart';

import 'atlas_theme.dart';

/// Both tab stacks stay alive (each keeps its navigation); switching cross-fades
/// with a short lift instead of cutting.
class FadeBranches extends StatelessWidget {
  const FadeBranches({super.key, required this.index, required this.children});
  final int index;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final d = Motion.reduced(context) ? Duration.zero : Motion.slow;
    return Stack(
      fit: StackFit.expand,
      children: [
        for (var i = 0; i < children.length; i++)
          IgnorePointer(
            ignoring: i != index,
            child: AnimatedOpacity(
              opacity: i == index ? 1 : 0,
              duration: d,
              curve: Motion.ease,
              child: AnimatedSlide(
                offset: i == index ? Offset.zero : const Offset(0, 0.02),
                duration: d,
                curve: Motion.ease,
                // Mute the hidden page's own animations, never the fade above:
                // muting that froze the page being left at full opacity.
                child: TickerMode(enabled: i == index, child: children[i]),
              ),
            ),
          ),
      ],
    );
  }
}
