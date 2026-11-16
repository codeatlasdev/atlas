import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// A controller that loops while [loopWhile] holds and motion is not reduced, and rests at
/// 0 otherwise. It re-checks on every dependency or widget change.
mixin MotionLoop<T extends StatefulWidget>
    on State<T>, SingleTickerProviderStateMixin<T> {
  Duration get loopDuration;
  bool get loopWhile;

  late final AnimationController loop = AnimationController(
    vsync: this,
    duration: loopDuration,
  );

  void syncLoop() {
    if (loopWhile && !Motion.reduced(context)) {
      if (!loop.isAnimating) loop.repeat();
    } else {
      loop.stop();
      loop.value = 0;
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    syncLoop();
  }

  @override
  void didUpdateWidget(covariant T oldWidget) {
    super.didUpdateWidget(oldWidget);
    syncLoop();
  }

  @override
  void dispose() {
    loop.dispose();
    super.dispose();
  }
}
