/// Three dots rising in turn: the "someone is typing / working" indicator.
library;

import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

class TypingDots extends StatefulWidget {
  const TypingDots({
    super.key,
    required this.color,
    this.size = 4,
    this.gap = 1.5,
    this.lift = 3,
    this.stagger = 0.15,
    this.stopWhenReduced = true,
  });

  final Color color;
  final double size;

  /// Horizontal margin on each side of a dot.
  final double gap;
  final double lift;

  /// Phase offset between neighbouring dots, as a fraction of the cycle.
  final double stagger;

  /// Hold still under reduced motion.
  final bool stopWhenReduced;

  @override
  State<TypingDots> createState() => _TypingDotsState();
}

class _TypingDotsState extends State<TypingDots>
    with SingleTickerProviderStateMixin {
  late final _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1100),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    widget.stopWhenReduced && Motion.reduced(context) ? _c.stop() : _c.repeat();
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _c,
    builder: (_, _) => Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var i = 0; i < 3; i++)
          Transform.translate(
            offset: Offset(
              0,
              -widget.lift *
                  math.max(
                    0,
                    math.sin((_c.value - i * widget.stagger) * 2 * math.pi),
                  ),
            ),
            child: Container(
              width: widget.size,
              height: widget.size,
              margin: EdgeInsets.symmetric(horizontal: widget.gap),
              decoration: BoxDecoration(
                color: widget.color,
                shape: BoxShape.circle,
              ),
            ),
          ),
      ],
    ),
  );
}
