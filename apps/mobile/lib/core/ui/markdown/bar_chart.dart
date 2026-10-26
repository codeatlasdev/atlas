import 'package:flutter/material.dart';

import '../atlas_theme.dart';

/// Compact horizontal bar chart: one row per label, bars scaled to the largest
/// absolute value. Rows are laid out by hand so the painter stays trivial.
class AtlasBarChart extends StatelessWidget {
  const AtlasBarChart({
    super.key,
    required this.title,
    required this.labels,
    required this.values,
    required this.unit,
  });

  final String title;
  final List<String> labels;
  final List<double> values;
  final String unit;

  static const _rowHeight = 22.0;

  static String format(double v, String unit) {
    final n = v == v.roundToDouble()
        ? v.round().toString()
        : v.toStringAsFixed(1);
    if (unit.isEmpty) return n;
    return unit == '%' ? '$n%' : '$n $unit';
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final small = TextStyle(color: c.muted, fontSize: 11.5, height: 1.2);
    final maxAbs = values.fold<double>(0, (m, v) => v.abs() > m ? v.abs() : m);
    return Container(
      key: const ValueKey('atlas-md-chart'),
      width: double.infinity,
      padding: const EdgeInsets.all(Space.md),
      decoration: BoxDecoration(
        color: c.base,
        borderRadius: BorderRadius.circular(Space.radiusSm),
        border: Border.all(color: c.borderSubtle),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title, style: small.copyWith(fontWeight: FontWeight.w600)),
          const SizedBox(height: Space.sm),
          for (var i = 0; i < values.length; i++)
            SizedBox(
              height: _rowHeight,
              child: Row(
                children: [
                  Flexible(
                    flex: 3,
                    child: Text(
                      labels[i],
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: small.copyWith(color: c.text),
                    ),
                  ),
                  const SizedBox(width: Space.sm),
                  Expanded(
                    flex: 5,
                    child: CustomPaint(
                      painter: _BarPainter(
                        fraction: maxAbs == 0 ? 0 : values[i].abs() / maxAbs,
                        color: c.series[i % c.series.length],
                        track: c.borderSubtle,
                      ),
                      child: const SizedBox.expand(),
                    ),
                  ),
                  const SizedBox(width: Space.sm),
                  Text(format(values[i], unit), style: small),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

class _BarPainter extends CustomPainter {
  const _BarPainter({
    required this.fraction,
    required this.color,
    required this.track,
  });

  final double fraction;
  final Color color;
  final Color track;

  @override
  void paint(Canvas canvas, Size size) {
    final bar = Rect.fromLTWH(0, size.height / 2 - 5, size.width, 10);
    final radius = const Radius.circular(5);
    canvas.drawRRect(
      RRect.fromRectAndRadius(bar, radius),
      Paint()..color = track,
    );
    final w = (size.width * fraction).clamp(
      fraction > 0 ? 3.0 : 0.0,
      size.width,
    );
    if (w > 0) {
      canvas.drawRRect(
        RRect.fromRectAndRadius(
          Rect.fromLTWH(0, bar.top, w, bar.height),
          radius,
        ),
        Paint()..color = color,
      );
    }
  }

  @override
  bool shouldRepaint(_BarPainter old) =>
      old.fraction != fraction || old.color != color || old.track != track;
}
