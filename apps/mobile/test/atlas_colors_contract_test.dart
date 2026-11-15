import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';

/// Atlas Dark as the desktop ships it. `flutter test` runs from `apps/mobile`.
Map<String, dynamic> _desktopDark() {
  final themes =
      (jsonDecode(
            File('../desktop/src/lib/themes/atlas.json').readAsStringSync(),
          ) as Map<String, dynamic>)['themes']
          as List;
  final dark = themes.cast<Map<String, dynamic>>().firstWhere(
    (t) => t['name'] == 'Atlas Dark',
  );
  return dark['style'] as Map<String, dynamic>;
}

/// `#rrggbb` or `#rrggbbaa` (CSS order) to a Flutter colour.
Color _hex(String css) {
  final h = css.substring(1);
  final rgb = int.parse(h.substring(0, 6), radix: 16);
  final a = h.length == 8 ? int.parse(h.substring(6), radix: 16) : 0xFF;
  return Color((a << 24) | rgb);
}

/// WCAG 2.x relative luminance and contrast ratio.
double _luminance(Color c) {
  double ch(double v) =>
      v <= 0.03928 ? v / 12.92 : math.pow((v + 0.055) / 1.055, 2.4).toDouble();
  return 0.2126 * ch(c.r) + 0.7152 * ch(c.g) + 0.0722 * ch(c.b);
}

double _contrast(Color fg, Color bg) {
  final a = _luminance(fg);
  final b = _luminance(bg);
  return (math.max(a, b) + 0.05) / (math.min(a, b) + 0.05);
}

void main() {
  const c = AtlasColors.dark;

  // Every AtlasColors.dark field that has a desktop counterpart, and the key it reads.
  final mapping = <String, (Color, String)>{
    'ground': (c.ground, 'background'),
    'base': (c.base, 'editor.background'),
    'raised': (c.raised, 'surface.background'),
    'overlay': (c.overlay, 'elevated_surface.background'),
    'text': (c.text, 'text'),
    'muted': (c.muted, 'text.muted'),
    'faint': (c.faint, 'text.disabled'),
    'accent': (c.accent, 'text.accent'),
    'onAccent': (c.onAccent, 'text.on_accent'),
    'border': (c.border, 'border'),
    'borderSubtle': (c.borderSubtle, 'border.variant'),
    'hover': (c.hover, 'element.hover'),
    'success': (c.success, 'state.success'),
    'warning': (c.warning, 'state.warning'),
    'info': (c.info, 'state.info'),
    'error': (c.error, 'state.error'),
  };

  // Regression: mobile kept a hand-copied palette and drifted from the desktop theme
  // (muted was #9AA7CC, the desktop's text.muted is #B9C4EB).
  test(
    'AtlasColors.dark equals the desktop Atlas Dark theme, colour by colour',
    () {
      final desktop = _desktopDark();
      final drift = <String>[];
      mapping.forEach((field, entry) {
        final (mobile, key) = entry;
        final want = _hex(desktop[key] as String);
        if (mobile.toARGB32() != want.toARGB32()) {
          drift.add(
            '$field: mobile #${mobile.toARGB32().toRadixString(16)} '
            '!= desktop "$key" ${desktop[key]}',
          );
        }
      });
      expect(drift, isEmpty, reason: drift.join('\n'));
    },
  );

  test('chart series are the desktop terminal ANSI blue, magenta, green, yellow, cyan, red', () {
    final desktop = _desktopDark();
    final want = [
      'blue',
      'magenta',
      'green',
      'yellow',
      'cyan',
      'red',
    ].map((n) => _hex(desktop['terminal.ansi.$n'] as String).toARGB32());
    expect(c.series.map((s) => s.toARGB32()), want);
  });

  // Regression: `faint` (4.12:1 on base) was used for timestamps, captions and labels.
  group('contrast (WCAG AA, 4.5:1 for small text)', () {
    final surfaces = {'ground': c.ground, 'base': c.base, 'raised': c.raised};
    final informative = {'text': c.text, 'muted': c.muted};

    for (final fg in informative.entries) {
      for (final bg in surfaces.entries) {
        test('${fg.key} on ${bg.key}', () {
          final ratio = _contrast(fg.value, bg.value);
          expect(
            ratio,
            greaterThanOrEqualTo(4.5),
            reason: '${fg.key} on ${bg.key} is ${ratio.toStringAsFixed(2)}:1',
          );
        });
      }
    }

    test(
      'faint stays under AA on base, so it must never carry readable text',
      () {
        // If this fails, faint became readable: the doc on AtlasColors.faint is stale.
        expect(_contrast(c.faint, c.base), lessThan(4.5));
      },
    );

    test(
      'faint still reads as a graphic (3:1, WCAG 1.4.11) on base and raised',
      () {
        expect(_contrast(c.faint, c.base), greaterThanOrEqualTo(3));
        expect(_contrast(c.faint, c.raised), greaterThanOrEqualTo(3));
      },
    );

    test('default small label colour is muted', () {
      expect(atlasTheme().textTheme.labelSmall?.color, c.muted);
    });
  });
}
