import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// One small line, scaled to its own max (service cards).
class Sparkline extends StatelessWidget {
  const Sparkline({super.key, required this.values, required this.color});
  final List<double> values;
  final Color color;

  @override
  Widget build(BuildContext context) => CustomPaint(
    size: Size.infinite,
    painter: _LinesPainter([values], [color], fill: true),
  );
}

/// Several series on one shared scale (the Resources chart).
class LinesChart extends StatelessWidget {
  const LinesChart({super.key, required this.series, required this.unit});
  final Map<String, List<double>> series;
  final String unit;

  @override
  Widget build(BuildContext context) {
    final names = series.keys.toList();
    final palette = AtlasColors.of(context).series;
    final colors = [
      for (var i = 0; i < names.length; i++) palette[i % palette.length],
    ];
    final max = series.values.expand((v) => v).fold(0.0, math.max);
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'max ${max.toStringAsFixed(max < 10 ? 1 : 0)} $unit',
          style: TextStyle(fontSize: 11, color: muted),
        ),
        const SizedBox(height: 4),
        CustomPaint(
          size: const Size(double.infinity, 140),
          painter: _LinesPainter([for (final n in names) series[n]!], colors),
        ),
        const SizedBox(height: 8),
        Wrap(
          spacing: 12,
          runSpacing: 4,
          children: [
            for (var i = 0; i < names.length; i++)
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Container(
                    width: 8,
                    height: 8,
                    decoration: BoxDecoration(
                      color: colors[i],
                      shape: BoxShape.circle,
                    ),
                  ),
                  const SizedBox(width: 4),
                  Text(names[i], style: TextStyle(fontSize: 11, color: muted)),
                ],
              ),
          ],
        ),
      ],
    );
  }
}

class _LinesPainter extends CustomPainter {
  _LinesPainter(this.series, this.colors, {this.fill = false});
  final List<List<double>> series;
  final List<Color> colors;
  final bool fill;

  @override
  void paint(Canvas canvas, Size size) {
    final max = series.expand((v) => v).fold(0.0, math.max);
    if (max <= 0) {
      final p = Paint()..color = colors.first.withValues(alpha: 0.3);
      canvas.drawLine(
        Offset(0, size.height - 1),
        Offset(size.width, size.height - 1),
        p,
      );
      return;
    }
    for (var i = 0; i < series.length; i++) {
      final v = series[i];
      if (v.isEmpty) continue;
      final step = v.length > 1 ? size.width / (v.length - 1) : size.width;
      final path = Path();
      for (var j = 0; j < v.length; j++) {
        final o = Offset(
          j * step,
          size.height - (v[j] / max) * (size.height - 2) - 1,
        );
        j == 0 ? path.moveTo(o.dx, o.dy) : path.lineTo(o.dx, o.dy);
      }
      if (fill) {
        final area = Path.from(path)
          ..lineTo((v.length - 1) * step, size.height)
          ..lineTo(0, size.height)
          ..close();
        canvas.drawPath(
          area,
          Paint()..color = colors[i].withValues(alpha: 0.18),
        );
      }
      canvas.drawPath(
        path,
        Paint()
          ..color = colors[i]
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5,
      );
    }
  }

  @override
  bool shouldRepaint(_LinesPainter old) => true;
}

/// One bar per service, each sample colored by its state (Health over time).
class HealthTimeline extends StatelessWidget {
  const HealthTimeline({super.key, required this.states});
  final Map<String, List<String>> states;

  @override
  Widget build(BuildContext context) {
    final muted = Theme.of(context).colorScheme.onSurfaceVariant;
    return Column(
      children: [
        for (final e in states.entries)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 3),
            child: Row(
              children: [
                SizedBox(
                  width: 72,
                  child: Text(
                    e.key,
                    style: TextStyle(fontSize: 11, color: muted),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                Expanded(
                  child: CustomPaint(
                    size: const Size(double.infinity, 8),
                    painter: _StatePainter(
                      e.value,
                      AtlasColors.of(context).serviceState,
                    ),
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

class _StatePainter extends CustomPainter {
  _StatePainter(this.states, this.colorOf);
  final List<String> states;
  final Color Function(String) colorOf;

  @override
  void paint(Canvas canvas, Size size) {
    if (states.isEmpty) return;
    final w = size.width / states.length;
    for (var i = 0; i < states.length; i++) {
      canvas.drawRect(
        Rect.fromLTWH(i * w, 0, w + 0.5, size.height),
        Paint()..color = colorOf(states[i]),
      );
    }
  }

  @override
  bool shouldRepaint(_StatePainter old) => true;
}
