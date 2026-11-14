import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:path_parsing/path_parsing.dart';

import 'atlas_theme.dart';

/// Bottom padding to add to scroll views so content isn't hidden under the bar.
const double kFloatingBarScrollPad = 96.0;

// ── SVG path helper (same pattern as agent_logo.dart) ─────────────────────

Path _svgPath(String d) {
  final p = Path();
  writeSvgPathDataToPath(d, _PathProxy(p));
  return p;
}

class _PathProxy extends PathProxy {
  _PathProxy(this._p);
  final Path _p;
  @override
  void moveTo(double x, double y) => _p.moveTo(x, y);
  @override
  void lineTo(double x, double y) => _p.lineTo(x, y);
  @override
  void cubicTo(
    double x1,
    double y1,
    double x2,
    double y2,
    double x3,
    double y3,
  ) => _p.cubicTo(x1, y1, x2, y2, x3, y3);
  @override
  void close() => _p.close();
}

// ── CodeXml icon (lucide code-2: </>)  viewBox 24×24 ─────────────────────

/// The lucide `code-2` / `</>` glyph: two angle brackets and a forward slash.
class CodeXmlIcon extends StatelessWidget {
  const CodeXmlIcon({super.key, required this.color, this.size = 24});

  final Color color;
  final double size;

  @override
  Widget build(BuildContext context) =>
      CustomPaint(size: Size(size, size), painter: _CodeXmlPainter(color));
}

class _CodeXmlPainter extends CustomPainter {
  const _CodeXmlPainter(this.color);
  final Color color;

  // Absolute-coordinate equivalents of lucide's relative paths:
  //   m18 16 4-4-4-4  →  M18,16 L22,12 L18,8   (right bracket >)
  //   m6 8-4 4 4 4    →  M6,8 L2,12 L6,16      (left bracket <)
  //   m14.5 4-5 16    →  M14.5,4 L9.5,20        (slash /)
  static final _right = _svgPath('M18 16 L22 12 L18 8');
  static final _left = _svgPath('M6 8 L2 12 L6 16');
  static final _slash = _svgPath('M14.5 4 L9.5 20');

  @override
  void paint(Canvas canvas, Size size) {
    final s = size.width / 24;
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      // Heavier than lucide's 2: sits beside filled Material glyphs in the bar.
      ..strokeWidth = 2.6 * s;

    canvas.save();
    canvas.scale(s, s);
    canvas.drawPath(_right, paint);
    canvas.drawPath(_left, paint);
    canvas.drawPath(_slash, paint);
    canvas.restore();
  }

  @override
  bool shouldRepaint(_CodeXmlPainter old) => old.color != color;
}

// ── ShipFlag icon (ShipFlag.svelte)  viewBox 32×32 ────────────────────────

/// The Atlas globe-with-straw-hat mark from ShipFlag.svelte.
/// [color] is `currentColor`; [accentColor] paints the hat band.
class ShipFlagIcon extends StatelessWidget {
  const ShipFlagIcon({
    super.key,
    required this.color,
    required this.accentColor,
    this.size = 32,
  });

  final Color color;
  final Color accentColor;
  final double size;

  @override
  Widget build(BuildContext context) => CustomPaint(
    size: Size(size, size),
    painter: _ShipFlagPainter(color: color, accentColor: accentColor),
  );
}

class _ShipFlagPainter extends CustomPainter {
  const _ShipFlagPainter({required this.color, required this.accentColor});
  final Color color;
  final Color accentColor;

  // Globe
  static final _longLeft = _svgPath(
    'M16 11 C12 13 10 17 10 20 C10 23 12 27 16 29',
  );
  static final _longRight = _svgPath(
    'M16 11 C20 13 22 17 22 20 C22 23 20 27 16 29',
  );
  static final _latitude = _svgPath('M7.2 20 Q16 17.5 24.8 20');
  // Hat
  static final _crownPath = _svgPath(
    'M10.5 13.5 C10.5 13.5 10.5 7.5 16 7.5 C21.5 7.5 21.5 13.5 21.5 13.5',
  );
  static final _band = _svgPath(
    'M11 12 C12.8 11.3 14.6 11 16 11 C17.4 11 19.2 11.3 21 12',
  );

  @override
  void paint(Canvas canvas, Size size) {
    final s = size.width / 32;
    canvas.save();
    canvas.scale(s, s);

    Paint stroke(Color c, double w) => Paint()
      ..color = c
      ..style = PaintingStyle.stroke
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..strokeWidth = w;

    Paint fill(Color c) => Paint()
      ..color = c
      ..style = PaintingStyle.fill;

    // Same visual weight as the `</>` glyph (2 px at 24): ~2.3 units at 32.
    const w = 2.3;
    const wThin = 1.7;

    // Globe circle
    canvas.drawCircle(const Offset(16, 20), 9, stroke(color, w));

    // Globe arcs
    canvas.drawPath(_longLeft, stroke(color, wThin));
    canvas.drawPath(_longRight, stroke(color, wThin));
    canvas.drawPath(_latitude, stroke(color, wThin));

    // Hat brim: narrower than the desktop mark so it reads at tab size.
    final brimRect = Rect.fromCenter(
      center: const Offset(16, 13.5),
      width: 17,
      height: 4.4,
    );
    canvas.drawOval(brimRect, fill(color.withValues(alpha: 0.15)));
    canvas.drawOval(brimRect, stroke(color, w));

    // Hat crown
    canvas.drawPath(_crownPath, fill(color.withValues(alpha: 0.12)));
    canvas.drawPath(_crownPath, stroke(color, w));

    // Hat band in accent colour
    canvas.drawPath(_band, stroke(accentColor, w));

    canvas.restore();
  }

  @override
  bool shouldRepaint(_ShipFlagPainter old) =>
      old.color != color || old.accentColor != accentColor;
}

// ── FloatingTabBar ─────────────────────────────────────────────────────────

/// The app's tabs, in bar order.
enum AtlasTab { computers, crew, inbox, settings }

/// Icon-only tab bar floating above content: a rounded slab, icons spread
/// evenly, the selected one on a lighter rounded square. Inbox with pending
/// items, while not selected, turns into an inverted pill with its count.
class FloatingTabBar extends StatelessWidget {
  const FloatingTabBar({
    super.key,
    required this.currentIndex,
    required this.onTabSelected,
    this.inboxCount = 0,
  });

  final int currentIndex;
  final ValueChanged<int> onTabSelected;

  /// Items waiting in the Inbox; 0 hides the pill.
  final int inboxCount;

  static const double _slot = 56.0;
  static const double _pad = 10.0;
  static const double _gap = 18.0;
  static const double _innerRadius = 16.0;
  // About 30% of the bar's height: rounded, but a slab, not a pill.
  static const double _outerRadius = 24.0;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    Widget item(AtlasTab tab, String label, Widget Function(Color) icon) =>
        _TabItem(
          index: tab.index,
          isActive: currentIndex == tab.index,
          onTap: onTabSelected,
          label: label,
          count: tab == AtlasTab.inbox ? inboxCount : 0,
          builder: icon,
        );

    return Center(
      child: Container(
        decoration: BoxDecoration(
          color: c.raised,
          borderRadius: BorderRadius.circular(_outerRadius),
          border: Border.all(color: c.border, width: 0.5),
          boxShadow: [
            BoxShadow(
              color: Colors.black.withValues(alpha: 0.35),
              blurRadius: 28,
              offset: const Offset(0, 10),
            ),
          ],
        ),
        padding: const EdgeInsets.all(_pad),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          spacing: _gap,
          children: [
            item(
              AtlasTab.computers,
              'Computers',
              (color) => CodeXmlIcon(color: color, size: 25),
            ),
            item(
              AtlasTab.crew,
              'Atlas Crew',
              (color) =>
                  ShipFlagIcon(color: color, accentColor: color, size: 30),
            ),
            item(
              AtlasTab.inbox,
              'Inbox',
              (color) =>
                  Icon(Icons.notifications_rounded, color: color, size: 27),
            ),
            item(
              AtlasTab.settings,
              'Settings',
              (color) => Icon(Icons.settings_rounded, color: color, size: 26),
            ),
          ],
        ),
      ),
    );
  }
}

// ── _TabItem ───────────────────────────────────────────────────────────────

class _TabItem extends StatefulWidget {
  const _TabItem({
    required this.index,
    required this.isActive,
    required this.onTap,
    required this.label,
    required this.builder,
    this.count = 0,
  });

  final int index;
  final bool isActive;
  final ValueChanged<int> onTap;
  final String label;
  final Widget Function(Color color) builder;
  final int count;

  @override
  State<_TabItem> createState() => _TabItemState();
}

class _TabItemState extends State<_TabItem> {
  bool _down = false;

  void _press(bool v) => setState(() => _down = v);

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final reduced = Motion.reduced(context);
    final d = reduced ? Duration.zero : Motion.slow;
    final badge = widget.count > 0 && !widget.isActive;
    final (bg, fg) = widget.isActive
        ? (Color.lerp(c.raised, c.text, 0.14)!, c.text)
        : (c.raised.withValues(alpha: 0), c.faint);

    return Semantics(
      label: widget.count > 0
          ? '${widget.label}, ${widget.count} waiting'
          : widget.label,
      button: true,
      selected: widget.isActive,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTapDown: (_) => _press(true),
        onTapCancel: () => _press(false),
        onTapUp: (_) => _press(false),
        onTap: () {
          if (!widget.isActive) HapticFeedback.selectionClick();
          widget.onTap(widget.index);
        },
        child: AnimatedScale(
          scale: _down ? 0.9 : 1.0,
          duration: reduced ? Duration.zero : Motion.fast,
          curve: Motion.ease,
          child: AnimatedContainer(
            duration: d,
            curve: Curves.easeOutCubic,
            height: FloatingTabBar._slot,
            constraints: const BoxConstraints(minWidth: FloatingTabBar._slot),
            width: FloatingTabBar._slot,
            decoration: BoxDecoration(
              color: bg,
              borderRadius: BorderRadius.circular(FloatingTabBar._innerRadius),
            ),
            // Unread count: a small badge on the icon's corner, popping in and out.
            child: Stack(
              clipBehavior: Clip.none,
              alignment: Alignment.center,
              children: [
                TweenAnimationBuilder<Color?>(
                  tween: ColorTween(end: fg),
                  duration: d,
                  curve: Motion.ease,
                  builder: (_, color, _) => widget.builder(color ?? fg),
                ),
                Positioned(
                  top: 9,
                  right: 7,
                  child: AnimatedScale(
                    // Hidden while the Inbox itself is open: you are reading it.
                    scale: badge ? 1 : 0,
                    duration: d,
                    curve: Curves.easeOutBack,
                    child: Container(
                      constraints: const BoxConstraints(minWidth: 18),
                      height: 18,
                      padding: const EdgeInsets.symmetric(horizontal: 5),
                      decoration: BoxDecoration(
                        color: c.accent,
                        borderRadius: BorderRadius.circular(9),
                        border: Border.all(color: c.raised, width: 2),
                      ),
                      alignment: Alignment.center,
                      child: Text(
                        !badge
                            ? ''
                            : widget.count > 99
                            ? '99+'
                            : '${widget.count}',
                        style: TextStyle(
                          color: c.onAccent,
                          fontSize: 10.5,
                          fontWeight: FontWeight.w800,
                          height: 1,
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
