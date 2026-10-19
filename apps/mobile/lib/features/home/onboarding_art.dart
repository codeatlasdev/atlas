import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// A looping clock for an illustration; still (at [still]) under reduced motion.
mixin _Loop<T extends StatefulWidget>
    on State<T>, SingleTickerProviderStateMixin<T> {
  Duration get period;
  double get still => 0;

  late final AnimationController loop = AnimationController(
    vsync: this,
    duration: period,
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (Motion.reduced(context)) {
      loop
        ..stop()
        ..value = still;
    } else if (!loop.isAnimating) {
      loop.repeat();
    }
  }

  @override
  void dispose() {
    loop.dispose();
    super.dispose();
  }
}

/// 0 → 1 over [a, b] of a 0..1 clock, eased.
double _seg(double t, double a, double b, [Curve curve = Motion.ease]) =>
    curve.transform(((t - a) / (b - a)).clamp(0.0, 1.0));

// ── Page 2: phone ⇄ computer, end-to-end encrypted, relay fallback ─────────────

/// The phone and the computer joined by a line; packets travel both ways through a lock
/// that closes as the first one passes. Every other loop the line is "blocked" and the
/// packets bend up through a relay instead: the content stays sealed either way.
class SecureLinkArt extends StatefulWidget {
  const SecureLinkArt({super.key});

  @override
  State<SecureLinkArt> createState() => _SecureLinkArtState();
}

class _SecureLinkArtState extends State<SecureLinkArt>
    with SingleTickerProviderStateMixin, _Loop {
  @override
  Duration get period => const Duration(milliseconds: 6400);

  @override
  double get still => .3;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return SizedBox(
      width: 280,
      height: 160,
      child: AnimatedBuilder(
        animation: loop,
        builder: (_, _) => CustomPaint(
          painter: _LinkPainter(t: loop.value, c: c),
          child: Stack(
            children: [
              Positioned(
                left: 0,
                top: 74,
                child: _Device(icon: Icons.phone_iphone_rounded, c: c),
              ),
              Positioned(
                right: 0,
                top: 74,
                child: _Device(icon: Icons.laptop_mac_rounded, c: c),
              ),
              Positioned(
                left: 120,
                top: 80,
                // Locks as the first packet reaches the middle of each trip, opens
                // between trips (same clock as _LinkPainter: two trips per half).
                child: _Lock(closed: (loop.value % .25 * 4) > .3, c: c),
              ),
              Positioned(
                left: 0,
                right: 0,
                top: 0,
                child: Opacity(
                  opacity: _relayShown(loop.value),
                  child: Column(
                    children: [
                      Icon(Icons.cloud_outlined, color: c.muted, size: 26),
                      Text(
                        'relay',
                        style: TextStyle(color: c.faint, fontSize: 10),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// The second half of the loop goes through the relay; it fades in and out around it.
double _relayShown(double t) =>
    t < .5 ? 0 : math.min(_seg(t, .5, .56), 1 - _seg(t, .94, 1));

class _Device extends StatelessWidget {
  const _Device({required this.icon, required this.c});
  final IconData icon;
  final AtlasColors c;

  @override
  Widget build(BuildContext context) => Container(
    width: 64,
    height: 64,
    decoration: BoxDecoration(
      color: c.base,
      borderRadius: BorderRadius.circular(18),
      border: Border.all(color: c.border),
    ),
    child: Icon(icon, color: c.text, size: 30),
  );
}

class _Lock extends StatelessWidget {
  const _Lock({required this.closed, required this.c});
  final bool closed;
  final AtlasColors c;

  @override
  Widget build(BuildContext context) => AnimatedContainer(
    duration: Motion.normal,
    curve: Motion.ease,
    width: 40,
    height: 40,
    decoration: BoxDecoration(
      color: (closed ? c.success : c.faint).withValues(alpha: .16),
      shape: BoxShape.circle,
    ),
    child: AnimatedSwitcher(
      duration: Motion.normal,
      transitionBuilder: (w, a) => ScaleTransition(scale: a, child: w),
      child: Icon(
        closed ? Icons.lock_rounded : Icons.lock_open_rounded,
        key: ValueKey(closed),
        size: 20,
        color: closed ? c.success : c.faint,
      ),
    ),
  );
}

class _LinkPainter extends CustomPainter {
  _LinkPainter({required this.t, required this.c});
  final double t;
  final AtlasColors c;

  @override
  void paint(Canvas canvas, Size size) {
    const a = Offset(64, 106), b = Offset(216, 106), relay = Offset(140, 26);
    final viaRelay = t >= .5;
    final half = t % .5 * 2; // 0..1 within each half

    // Path: straight, or bent up through the relay.
    final path = Path()..moveTo(a.dx, a.dy);
    if (viaRelay) {
      path.quadraticBezierTo(a.dx + 20, relay.dy + 10, relay.dx, relay.dy + 22);
      path.quadraticBezierTo(b.dx - 20, relay.dy + 10, b.dx, b.dy);
    } else {
      path.lineTo(b.dx, b.dy);
    }
    // Direct line dims while the relay carries the traffic ("blocked").
    final line = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2
      ..color = c.border;
    if (viaRelay) {
      final blocked = Paint()
        ..strokeWidth = 2
        ..color = c.error.withValues(alpha: .35 * _relayShown(t));
      for (var x = a.dx; x < b.dx; x += 10) {
        canvas.drawLine(
          Offset(x, a.dy),
          Offset(math.min(x + 5, b.dx), a.dy),
          blocked,
        );
      }
    }
    canvas.drawPath(path, line);

    // Packets: three, staggered. First half of each round phone → computer, second half
    // computer → phone; each trip runs the whole line (the last packet lands at .95).
    final metric = path.computeMetrics().first;
    final forward = half < .5;
    final trip = forward
        ? half * 2
        : (half - .5) * 2; // 0..1 within one direction
    for (var i = 0; i < 3; i++) {
      final p = ((trip - i * .15) / .65).clamp(0.0, 1.0);
      if (p <= 0 || p >= 1) continue;
      final pos = metric
          .getTangentForOffset(metric.length * (forward ? p : 1 - p))!
          .position;
      final glow = Paint()..color = c.accent.withValues(alpha: .25);
      canvas.drawCircle(pos, 7, glow);
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          Rect.fromCenter(center: pos, width: 9, height: 9),
          const Radius.circular(2),
        ),
        Paint()..color = c.accent,
      );
    }
  }

  @override
  bool shouldRepaint(_LinkPainter old) => old.t != t;
}

// ── Page 3: an agent card living through a turn ─────────────────────────────────

/// working → needs your approval → (Allow pressed) → working → done, on loop.
class AgentStoryArt extends StatefulWidget {
  const AgentStoryArt({super.key});

  @override
  State<AgentStoryArt> createState() => _AgentStoryArtState();
}

class _AgentStoryArtState extends State<AgentStoryArt>
    with SingleTickerProviderStateMixin, _Loop {
  @override
  Duration get period => const Duration(milliseconds: 7000);

  @override
  double get still => .4;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return AnimatedBuilder(
      animation: loop,
      builder: (_, _) {
        final t = loop.value;
        final stage = t < .22
            ? 0 // working
            : t < .55
            ? 1 // needs you
            : t < .75
            ? 2 // working again
            : 3; // done
        final press = _seg(t, .44, .5) * (1 - _seg(t, .5, .55));
        final (label, color) = switch (stage) {
          1 => ('needs you', c.warning),
          3 => ('done', c.success),
          _ => ('working', c.accent),
        };
        return SizedBox(
          width: 280,
          child: Container(
            padding: const EdgeInsets.all(Space.md),
            decoration: BoxDecoration(
              color: c.base,
              borderRadius: BorderRadius.circular(Space.radius),
              border: Border.all(
                color: stage == 1
                    ? c.warning.withValues(alpha: .6)
                    : c.borderSubtle,
              ),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    _Pulse(color: color, on: stage != 3, t: t),
                    const SizedBox(width: 8),
                    const Text(
                      'claude',
                      style: TextStyle(fontWeight: FontWeight.w600),
                    ),
                    Expanded(
                      child: Text(
                        '  · orbit-shop',
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(color: c.faint, fontSize: 12),
                      ),
                    ),
                    AnimatedSwitcher(
                      duration: Motion.normal,
                      child: Text(
                        label,
                        key: ValueKey(label),
                        style: TextStyle(color: color, fontSize: 12),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: Space.sm),
                AnimatedSize(
                  duration: Motion.slow,
                  curve: Motion.ease,
                  alignment: Alignment.topCenter,
                  child: AnimatedSwitcher(
                    duration: Motion.slow,
                    switchInCurve: Motion.ease,
                    transitionBuilder: (w, a) => FadeTransition(
                      opacity: a,
                      child: SlideTransition(
                        position: Tween(
                          begin: const Offset(0, .15),
                          end: Offset.zero,
                        ).animate(a),
                        child: w,
                      ),
                    ),
                    child: switch (stage) {
                      1 => Column(
                        key: const ValueKey('ask'),
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Wants to run',
                            style: TextStyle(color: c.muted, fontSize: 12),
                          ),
                          const SizedBox(height: 4),
                          Text(
                            'bun run db:migrate',
                            style: TextStyle(
                              fontFamily: monoFamily,
                              fontFamilyFallback: monoFallback,
                              color: c.text,
                              fontSize: 13,
                            ),
                          ),
                          const SizedBox(height: Space.sm),
                          Row(
                            mainAxisAlignment: MainAxisAlignment.end,
                            children: [
                              _FakeButton('Deny', filled: false, c: c),
                              const SizedBox(width: 8),
                              Transform.scale(
                                scale: 1 - .08 * press,
                                child: _FakeButton('Allow', filled: true, c: c),
                              ),
                            ],
                          ),
                        ],
                      ),
                      3 => Row(
                        key: const ValueKey('done'),
                        children: [
                          Icon(
                            Icons.check_circle_rounded,
                            color: c.success,
                            size: 18,
                          ),
                          const SizedBox(width: 6),
                          Expanded(
                            child: Text(
                              'Migration applied. 840 ms → 9 ms.',
                              style: TextStyle(color: c.muted, fontSize: 13),
                            ),
                          ),
                        ],
                      ),
                      _ => _Typing(key: const ValueKey('typing'), t: t, c: c),
                    },
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class _Pulse extends StatelessWidget {
  const _Pulse({required this.color, required this.on, required this.t});
  final Color color;
  final bool on;
  final double t;

  @override
  Widget build(BuildContext context) {
    final p = (t * 5) % 1;
    return SizedBox.square(
      dimension: 16,
      child: Stack(
        alignment: Alignment.center,
        children: [
          if (on)
            Container(
              width: 8 + 8 * p,
              height: 8 + 8 * p,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: color.withValues(alpha: .35 * (1 - p)),
              ),
            ),
          AnimatedContainer(
            duration: Motion.normal,
            width: 8,
            height: 8,
            decoration: BoxDecoration(shape: BoxShape.circle, color: color),
          ),
        ],
      ),
    );
  }
}

class _Typing extends StatelessWidget {
  const _Typing({super.key, required this.t, required this.c});
  final double t;
  final AtlasColors c;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      for (var i = 0; i < 3; i++)
        Transform.translate(
          offset: Offset(
            0,
            -3 * math.sin(((t * 6 - i * .18) % 1) * math.pi).clamp(0, 1),
          ),
          child: Container(
            margin: const EdgeInsets.only(right: 4, top: 6, bottom: 6),
            width: 6,
            height: 6,
            decoration: BoxDecoration(color: c.muted, shape: BoxShape.circle),
          ),
        ),
    ],
  );
}

class _FakeButton extends StatelessWidget {
  const _FakeButton(this.label, {required this.filled, required this.c});
  final String label;
  final bool filled;
  final AtlasColors c;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 7),
    decoration: BoxDecoration(
      color: filled ? c.accent : null,
      border: filled ? null : Border.all(color: c.border),
      borderRadius: BorderRadius.circular(Space.radiusSm),
    ),
    child: Text(
      label,
      style: TextStyle(
        color: filled ? c.onAccent : c.text,
        fontSize: 13,
        fontWeight: FontWeight.w600,
      ),
    ),
  );
}

// ── Page 4: a pairing code being scanned ────────────────────────────────────────

/// A QR code in a frame with a scan line sweeping down; a check pops when it reaches
/// the bottom.
class ScanArt extends StatefulWidget {
  const ScanArt({super.key});

  @override
  State<ScanArt> createState() => _ScanArtState();
}

class _ScanArtState extends State<ScanArt>
    with SingleTickerProviderStateMixin, _Loop {
  @override
  Duration get period => const Duration(milliseconds: 3200);

  @override
  double get still => .85;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return AnimatedBuilder(
      animation: loop,
      builder: (_, _) {
        final t = loop.value;
        final sweep = _seg(t, .05, .65, Curves.easeInOut);
        final ok =
            _seg(t, .68, .8, const Cubic(.2, .9, .3, 1.25)) *
            (1 - _seg(t, .94, 1));
        return SizedBox.square(
          dimension: 120,
          child: CustomPaint(
            painter: _QrPainter(sweep: t < .66 ? sweep : null, c: c),
            child: Center(
              child: Opacity(
                opacity: ok.clamp(0.0, 1.0),
                child: Transform.scale(
                  scale: .5 + .5 * ok,
                  child: Container(
                    width: 44,
                    height: 44,
                    decoration: BoxDecoration(
                      color: c.success,
                      shape: BoxShape.circle,
                    ),
                    child: Icon(
                      Icons.check_rounded,
                      color: c.onAccent,
                      size: 28,
                    ),
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

class _QrPainter extends CustomPainter {
  _QrPainter({required this.sweep, required this.c});
  final double? sweep;
  final AtlasColors c;

  /// A fixed pseudo-QR: three finder squares and a deterministic module pattern.
  static final _modules = [
    for (var y = 0; y < 13; y++)
      for (var x = 0; x < 13; x++)
        if (!((x < 4 && y < 4) || (x > 8 && y < 4) || (x < 4 && y > 8)) &&
            (x * 7 + y * 13 + x * y) % 3 == 0)
          (x, y),
  ];

  @override
  void paint(Canvas canvas, Size size) {
    const inset = 14.0;
    final cell = (size.width - inset * 2) / 13;
    final ink = Paint()..color = c.text.withValues(alpha: .85);
    void finder(int x, int y) {
      final r = Rect.fromLTWH(
        inset + x * cell,
        inset + y * cell,
        cell * 3,
        cell * 3,
      );
      canvas.drawRRect(
        RRect.fromRectAndRadius(r, Radius.circular(cell * .6)),
        ink
          ..style = PaintingStyle.stroke
          ..strokeWidth = cell * .7,
      );
      canvas.drawRect(r.deflate(cell), ink..style = PaintingStyle.fill);
    }

    finder(0, 0);
    finder(10, 0);
    finder(0, 10);
    for (final (x, y) in _modules) {
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          Rect.fromLTWH(
            inset + x * cell + .5,
            inset + y * cell + .5,
            cell - 1,
            cell - 1,
          ),
          const Radius.circular(1.5),
        ),
        ink,
      );
    }
    // Frame corners.
    final frame = Paint()
      ..color = c.accent
      ..style = PaintingStyle.stroke
      ..strokeWidth = 3
      ..strokeCap = StrokeCap.round;
    const l = 18.0, r = 4.0;
    for (final (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]) {
      final x = dx * size.width + (dx == 0 ? r : -r),
          y = dy * size.height + (dy == 0 ? r : -r);
      final sx = dx == 0 ? 1.0 : -1.0, sy = dy == 0 ? 1.0 : -1.0;
      canvas.drawLine(Offset(x, y), Offset(x + sx * l, y), frame);
      canvas.drawLine(Offset(x, y), Offset(x, y + sy * l), frame);
    }
    // Scan line with a soft trail.
    final s = sweep;
    if (s != null) {
      final y = inset + (size.height - inset * 2) * s;
      final trail = Rect.fromLTRB(inset, y - 22, size.width - inset, y);
      canvas.drawRect(
        trail,
        Paint()
          ..shader = LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [
              c.accent.withValues(alpha: 0),
              c.accent.withValues(alpha: .28),
            ],
          ).createShader(trail),
      );
      canvas.drawLine(
        Offset(inset - 4, y),
        Offset(size.width - inset + 4, y),
        Paint()
          ..color = c.accent
          ..strokeWidth = 2,
      );
    }
  }

  @override
  bool shouldRepaint(_QrPainter old) => old.sweep != sweep;
}
