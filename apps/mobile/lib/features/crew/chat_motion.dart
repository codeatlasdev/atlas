/// Motion for the crew chat: messages that arrive instead of appearing, and a press that gives.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// A message arriving: it slides in from its sender's side, rises a little and settles
/// with a soft overshoot. Plays once, when [play] is set; otherwise it is simply there.
class ChatAppear extends StatefulWidget {
  const ChatAppear({
    super.key,
    required this.child,
    required this.fromRight,
    this.play = true,
    this.delay = Duration.zero,
  });

  final Widget child;
  final bool fromRight;
  final bool play;
  final Duration delay;

  @override
  State<ChatAppear> createState() => _ChatAppearState();
}

class _ChatAppearState extends State<ChatAppear>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 460),
    value: widget.play ? 0 : 1,
  );
  late final _move = CurvedAnimation(parent: _c, curve: Curves.easeOutCubic);
  late final _pop = CurvedAnimation(parent: _c, curve: Curves.easeOutBack);

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_c.value == 1) return;
    if (Motion.reduced(context)) {
      _c.value = 1;
    } else if (!_c.isAnimating) {
      Future.delayed(widget.delay, () {
        if (mounted) _c.forward();
      });
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _c,
      child: widget.child,
      builder: (_, child) {
        final t = _move.value;
        final side = widget.fromRight ? 1.0 : -1.0;
        return Opacity(
          opacity: t.clamp(0, 1),
          child: Transform.translate(
            offset: Offset(side * 28 * (1 - t), 14 * (1 - t)),
            child: Transform.scale(
              scale: 0.9 + 0.1 * _pop.value,
              alignment: widget.fromRight
                  ? Alignment.bottomRight
                  : Alignment.bottomLeft,
              child: child,
            ),
          ),
        );
      },
    );
  }
}

/// Gives under the finger: scales down while pressed, springs back on release.
class Pressable extends StatefulWidget {
  const Pressable({super.key, required this.child, required this.onTap});
  final Widget child;
  final VoidCallback? onTap;

  @override
  State<Pressable> createState() => _PressableState();
}

class _PressableState extends State<Pressable> {
  bool _down = false;

  void _set(bool v) {
    if (widget.onTap != null) setState(() => _down = v);
  }

  @override
  Widget build(BuildContext context) => GestureDetector(
    behavior: HitTestBehavior.opaque,
    onTapDown: (_) => _set(true),
    onTapUp: (_) => _set(false),
    onTapCancel: () => _set(false),
    onTap: widget.onTap,
    child: AnimatedScale(
      scale: _down ? 0.88 : 1,
      duration: _down ? const Duration(milliseconds: 90) : Motion.slow,
      curve: _down ? Curves.easeOut : Curves.elasticOut,
      child: widget.child,
    ),
  );
}
