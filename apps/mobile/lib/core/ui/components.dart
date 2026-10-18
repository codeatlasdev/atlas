import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// The desktop's panel card: base fill, hairline border, 12 px radius. Pressable when
/// [onTap] is set (subtle scale + hover fill).
class AtlasCard extends StatefulWidget {
  const AtlasCard({
    super.key,
    required this.child,
    this.onTap,
    this.padding = const EdgeInsets.all(Space.md),
    this.margin = const EdgeInsets.only(bottom: Space.sm),
    this.accent,
  });

  final Widget child;
  final VoidCallback? onTap;
  final EdgeInsets padding;
  final EdgeInsets margin;

  /// Coloured left edge (e.g. a failed service, an agent that needs you).
  final Color? accent;

  @override
  State<AtlasCard> createState() => _AtlasCardState();
}

class _AtlasCardState extends State<AtlasCard> {
  bool _down = false;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final card = AnimatedContainer(
      duration: Motion.fast,
      curve: Motion.ease,
      margin: widget.margin,
      width: double.infinity,
      decoration: BoxDecoration(
        color: _down ? Color.alphaBlend(c.hover, c.base) : c.base,
        borderRadius: BorderRadius.circular(Space.radius),
        border: Border.all(color: c.borderSubtle),
      ),
      clipBehavior: Clip.antiAlias,
      child: Stack(
        children: [
          Padding(padding: widget.padding, child: widget.child),
          if (widget.accent != null)
            Positioned(
              left: 0,
              top: 0,
              bottom: 0,
              child: Container(width: 3, color: widget.accent),
            ),
        ],
      ),
    );
    if (widget.onTap == null) return card;
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTapDown: (_) => setState(() => _down = true),
      onTapCancel: () => setState(() => _down = false),
      onTapUp: (_) => setState(() => _down = false),
      onTap: widget.onTap,
      child: AnimatedScale(
        scale: _down ? .985 : 1,
        duration: Motion.fast,
        curve: Motion.ease,
        child: card,
      ),
    );
  }
}

/// A state dot; [pulse] adds the desktop's breathing halo (running work, waiting on you).
class StatusDot extends StatefulWidget {
  const StatusDot({
    super.key,
    required this.color,
    this.pulse = false,
    this.size = 8,
  });

  final Color color;
  final bool pulse;
  final double size;

  @override
  State<StatusDot> createState() => _StatusDotState();
}

class _StatusDotState extends State<StatusDot>
    with SingleTickerProviderStateMixin {
  late final AnimationController _a = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1400),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(StatusDot old) {
    super.didUpdateWidget(old);
    _sync();
  }

  void _sync() {
    if (widget.pulse && !Motion.reduced(context)) {
      if (!_a.isAnimating) _a.repeat();
    } else {
      _a.stop();
      _a.value = 0;
    }
  }

  @override
  void dispose() {
    _a.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final s = widget.size;
    return SizedBox.square(
      dimension: s * 2.2,
      child: AnimatedBuilder(
        animation: _a,
        builder: (_, _) => Stack(
          alignment: Alignment.center,
          children: [
            if (widget.pulse)
              Container(
                width: s * (1 + 1.2 * _a.value),
                height: s * (1 + 1.2 * _a.value),
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: widget.color.withValues(alpha: .35 * (1 - _a.value)),
                ),
              ),
            AnimatedContainer(
              duration: Motion.normal,
              width: s,
              height: s,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: widget.color,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Uppercase-free section title with an optional count and trailing control.
class SectionHeader extends StatelessWidget {
  const SectionHeader(this.title, {super.key, this.count, this.trailing});

  final String title;
  final int? count;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(2, Space.lg, 2, Space.sm),
      child: Row(
        children: [
          Text(title, style: Theme.of(context).textTheme.titleSmall),
          if (count != null) ...[
            const SizedBox(width: 6),
            Text('$count', style: TextStyle(color: c.faint, fontSize: 13)),
          ],
          const Spacer(),
          ?trailing,
        ],
      ),
    );
  }
}

/// Centered logo, title, line and action, used for every empty and error state.
class EmptyState extends StatelessWidget {
  const EmptyState({
    super.key,
    required this.title,
    required this.message,
    this.action,
    this.icon,
  });

  final String title;
  final String message;
  final Widget? action;

  /// Defaults to the Atlas logo.
  final Widget? icon;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(Space.xxl),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            icon ?? const Opacity(opacity: .9, child: AtlasLogo(size: 72)),
            const SizedBox(height: Space.xl),
            Text(
              title,
              style: Theme.of(context).textTheme.titleMedium,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: Space.sm),
            Text(
              message,
              style: TextStyle(color: c.muted, height: 1.4),
              textAlign: TextAlign.center,
            ),
            if (action != null) ...[const SizedBox(height: Space.xl), action!],
          ],
        ),
      ),
    );
  }
}

/// Fades and lifts its child in, [index] × [Motion.stagger] after first build.
class EnterFade extends StatefulWidget {
  const EnterFade({super.key, required this.child, this.index = 0});

  final Widget child;
  final int index;

  @override
  State<EnterFade> createState() => _EnterFadeState();
}

class _EnterFadeState extends State<EnterFade>
    with SingleTickerProviderStateMixin {
  late final AnimationController _a = AnimationController(
    vsync: this,
    duration: Motion.slow,
  );

  @override
  void initState() {
    super.initState();
    Future.delayed(Motion.stagger * widget.index.clamp(0, 12), () {
      if (mounted) _a.forward();
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (Motion.reduced(context)) _a.value = 1;
  }

  @override
  void dispose() {
    _a.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = CurvedAnimation(parent: _a, curve: Motion.ease);
    return FadeTransition(
      opacity: t,
      child: SlideTransition(
        position: Tween(
          begin: const Offset(0, .04),
          end: Offset.zero,
        ).animate(t),
        child: widget.child,
      ),
    );
  }
}

/// A rounded tinted pill: state labels, ports, counts.
class Pill extends StatelessWidget {
  const Pill(
    this.label, {
    super.key,
    required this.color,
    this.mono = false,
    this.onTap,
  });

  final String label;
  final Color color;
  final bool mono;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final pill = Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
      decoration: BoxDecoration(
        color: color.withValues(alpha: .14),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Text(
        label,
        style: TextStyle(
          color: color,
          fontSize: 12,
          fontWeight: FontWeight.w600,
          fontFamily: mono ? monoFamily : null,
          fontFamilyFallback: mono ? monoFallback : null,
        ),
      ),
    );
    return onTap == null ? pill : GestureDetector(onTap: onTap, child: pill);
  }
}
