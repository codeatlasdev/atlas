import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:path_parsing/path_parsing.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/logo_data.dart';

/// The Atlas globe. [animated]: the desktop's entrance (each tile pops in on its own delay)
/// followed by the slow opacity wave; static and fully drawn under reduced motion.
class AtlasLogo extends StatefulWidget {
  const AtlasLogo({super.key, this.size = 64, this.animated = false});

  final double size;
  final bool animated;

  @override
  State<AtlasLogo> createState() => _AtlasLogoState();
}

/// Tiles parsed once for the whole app.
final List<(Path, double, double)> _tiles = [
  for (final (d, delay, wave) in logoTiles) (_parse(d), delay, wave),
];

Path _parse(String d) {
  final p = Path();
  writeSvgPathDataToPath(d, _PathProxy(p));
  return p;
}

class _PathProxy extends PathProxy {
  _PathProxy(this.p);
  final Path p;
  @override
  void moveTo(double x, double y) => p.moveTo(x, y);
  @override
  void lineTo(double x, double y) => p.lineTo(x, y);
  @override
  void cubicTo(
    double x1,
    double y1,
    double x2,
    double y2,
    double x3,
    double y3,
  ) => p.cubicTo(x1, y1, x2, y2, x3, y3);
  @override
  void close() => p.close();
}

class _AtlasLogoState extends State<AtlasLogo>
    with SingleTickerProviderStateMixin {
  late final AnimationController _t = AnimationController(
    vsync: this,
    duration: const Duration(days: 1),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (widget.animated && !Motion.reduced(context) && !_t.isAnimating) {
      _t.forward();
    }
  }

  @override
  void dispose() {
    _t.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => RepaintBoundary(
    child: AnimatedBuilder(
      animation: _t,
      builder: (_, _) => CustomPaint(
        size: Size.square(widget.size),
        painter: _LogoPainter(
          seconds: widget.animated && !Motion.reduced(context)
              ? _t.lastElapsedDuration?.inMicroseconds.toDouble() ?? 0
              : null,
        ),
      ),
    ),
  );
}

const _pop = Cubic(.2, .9, .3, 1.25);

class _LogoPainter extends CustomPainter {
  _LogoPainter({required double? seconds})
    : t = seconds == null ? null : seconds / 1e6;

  /// Seconds since the entrance started; null = static, fully drawn.
  final double? t;

  static const _view = Rect.fromLTWH(0, 0, 428, 427);
  static final _shader = const LinearGradient(
    begin: Alignment(-0.81, -0.91),
    end: Alignment(0.82, 0.92),
    colors: [Color(0xFFBFD0FF), Color(0xFF3D5BFF), Color(0xFF1B33F0)],
    stops: [0, .45, 1],
  ).createShader(_view);

  @override
  void paint(Canvas canvas, Size size) {
    canvas.scale(size.width / _view.width, size.height / _view.height);
    for (final (path, delay, wave) in _tiles) {
      var scale = 1.0, opacity = 1.0;
      final now = t;
      if (now != null) {
        final p = ((now - delay) / .6).clamp(0.0, 1.0);
        final e = _pop.transform(p);
        scale = .55 + .45 * e;
        opacity = p == 0 ? 0 : e.clamp(0.0, 1.0);
        // Wave: 3.2 s cycle starting at 1.8 s + offset; dips to .78 at 35 %.
        final w = now - 1.8 - wave;
        if (w > 0 && p >= 1) {
          final phase = (w % 3.2) / 3.2;
          final dip = phase < .35
              ? phase / .35
              : phase < .7
              ? 1 - (phase - .35) / .35
              : 0.0;
          opacity = 1 - .22 * math.sin(dip * math.pi / 2);
        }
      }
      if (opacity <= 0) continue;
      final b = path.getBounds();
      canvas.save();
      canvas.translate(b.center.dx, b.center.dy);
      canvas.scale(scale);
      canvas.translate(-b.center.dx, -b.center.dy);
      canvas.drawPath(
        path,
        Paint()
          ..shader = _shader
          ..color = Color.fromRGBO(0, 0, 0, opacity),
      );
      canvas.restore();
    }
  }

  @override
  bool shouldRepaint(_LogoPainter old) => old.t != t;
}
