import 'package:flutter/material.dart';
import 'package:xterm/xterm.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// The Mac's terminal exactly as it is, every column and row, scaled down so its
/// full width fits the phone. Pinch, or double-tap a spot, to zoom into it.
///
/// This is what a full-screen agent (Claude, omp) needs: its screen is a picture
/// laid out for the Mac's size; reflowing it to the phone would tear it apart.
class LiveScreen extends StatefulWidget {
  const LiveScreen({super.key, required this.terminal, required this.theme});
  final Terminal terminal;
  final TerminalTheme theme;

  /// Width of one cell per point of font size, measured once for the mono font.
  static final double _cellRatio = () {
    final p = TextPainter(
      text: const TextSpan(
        text: 'MMMMMMMMMM',
        style: TextStyle(
          fontFamily: monoFamily,
          fontFamilyFallback: monoFallback,
          fontSize: 10,
        ),
      ),
      textDirection: TextDirection.ltr,
    )..layout();
    return p.width / 100;
  }();

  /// Font size at which [cols] cells fill [width].
  static double fitFont(double width, int cols) =>
      (width / (cols * _cellRatio)).clamp(3.0, 14.0);

  @override
  State<LiveScreen> createState() => _LiveScreenState();
}

class _LiveScreenState extends State<LiveScreen> {
  final _zoom = TransformationController();
  Offset _tap = Offset.zero;

  @override
  void dispose() {
    _zoom.dispose();
    super.dispose();
  }

  // Double-tap: zoom 2.5x around the tapped point, or back out.
  void _toggleZoom() {
    if (_zoom.value.getMaxScaleOnAxis() > 1.01) {
      _zoom.value = Matrix4.identity();
      return;
    }
    const s = 2.5;
    _zoom.value = Matrix4.identity()
      ..translateByDouble(-_tap.dx * (s - 1), -_tap.dy * (s - 1), 0, 1)
      ..scaleByDouble(s, s, 1, 1);
  }

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, box) {
      final font = LiveScreen.fitFont(
        box.maxWidth - 8,
        widget.terminal.viewWidth,
      );
      return GestureDetector(
        onDoubleTapDown: (d) => _tap = d.localPosition,
        onDoubleTap: _toggleZoom,
        child: InteractiveViewer(
          transformationController: _zoom,
          minScale: 1,
          maxScale: 5,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 4),
            child: TerminalView(
              widget.terminal,
              // The size is the Mac's; the phone never resizes the pty.
              autoResize: false,
              readOnly: true,
              theme: widget.theme,
              textStyle: TerminalStyle(
                fontSize: font,
                fontFamily: monoFamily,
                fontFamilyFallback: monoFallback,
              ),
              backgroundOpacity: 1,
            ),
          ),
        ),
      );
    },
  );
}
