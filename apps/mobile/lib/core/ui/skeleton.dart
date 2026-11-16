import 'package:flutter/material.dart';

import 'atlas_theme.dart';

/// A loading screen drawn as the screen it will become: [Skeleton] runs one light sweep for every
/// [SkeletonBlock] under it, so the whole layout breathes together instead of a spinner in the middle.
/// Under Reduce motion the blocks hold still.
class Skeleton extends StatefulWidget {
  const Skeleton({super.key, required this.child});
  final Widget child;

  @override
  State<Skeleton> createState() => _SkeletonState();
}

class _SkeletonState extends State<Skeleton>
    with SingleTickerProviderStateMixin {
  late final AnimationController _sweep = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1400),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (Motion.reduced(context)) {
      _sweep
        ..stop()
        ..value = .5;
    } else if (!_sweep.isAnimating) {
      _sweep.repeat();
    }
  }

  @override
  void dispose() {
    _sweep.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Semantics(
    label: 'Loading',
    child: ExcludeSemantics(
      child: _Sweep(animation: _sweep, child: widget.child),
    ),
  );
}

class _Sweep extends InheritedWidget {
  const _Sweep({required this.animation, required super.child});
  final Animation<double> animation;

  @override
  bool updateShouldNotify(_Sweep old) => old.animation != animation;
}

/// One placeholder shape: a line of text, an avatar, a card. Sized like what it stands for.
class SkeletonBlock extends StatelessWidget {
  const SkeletonBlock({
    super.key,
    this.width,
    this.height = 12,
    this.radius = 6,
    this.circle = false,
  });

  final double? width;
  final double height;
  final double radius;
  final bool circle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final sweep = context
        .dependOnInheritedWidgetOfExactType<_Sweep>()
        ?.animation;
    final shape = circle
        ? const BoxDecoration(shape: BoxShape.circle)
        : BoxDecoration(borderRadius: BorderRadius.circular(radius));
    Widget box(double t) => Container(
      width: circle ? height : width,
      height: height,
      decoration: shape.copyWith(
        gradient: LinearGradient(
          begin: Alignment(-1.6 + 3.2 * t, -.2),
          end: Alignment(-.6 + 3.2 * t, .2),
          colors: [c.raised, c.overlay, c.raised],
          stops: const [0, .5, 1],
        ),
      ),
    );
    if (sweep == null) return box(.5);
    return AnimatedBuilder(
      animation: sweep,
      builder: (_, _) => box(sweep.value),
    );
  }
}

/// A card-shaped placeholder with the app card's fill, border and radius around [child].
class SkeletonCard extends StatelessWidget {
  const SkeletonCard({super.key, required this.child, this.padding});
  final Widget child;
  final EdgeInsetsGeometry? padding;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      margin: const EdgeInsets.only(bottom: Space.sm),
      padding: padding ?? const EdgeInsets.all(Space.lg),
      decoration: BoxDecoration(
        color: c.base,
        borderRadius: BorderRadius.circular(Space.radius),
        border: Border.all(color: c.borderSubtle),
      ),
      child: child,
    );
  }
}
