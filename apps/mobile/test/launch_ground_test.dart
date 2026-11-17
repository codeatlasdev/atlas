import 'dart:io';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';

/// The launch is one picture: the OS shows the ground, Flutter draws its first frame on the same ground, and
/// the globe forms on it. A native colour that drifts from `AtlasColors.dark.ground` is a visible flash
/// between the two (black on Android, a shifted blue on iOS), so every native file is checked against the
/// Dart colour, the one source. `flutter test` runs from `apps/mobile`.
void main() {
  final ground = AtlasColors.dark.ground;
  final hex =
      '#${(ground.toARGB32() & 0xFFFFFF).toRadixString(16).padLeft(6, '0')}';

  String read(String path) => File(path).readAsStringSync();
  const res = 'android/app/src/main/res';

  /// `#rrggbb` (or `#aarrggbb`) from an Android colour value to an opaque Flutter colour.
  Color androidColor(String v) {
    final h = v.trim().substring(1);
    return Color(0xFF000000 | int.parse(h.substring(h.length - 6), radix: 16));
  }

  /// The value of `<color name="atlas_ground">` in values/colors.xml.
  String groundXml() {
    final m = RegExp(r'<color name="atlas_ground">([^<]+)</color>')
        .firstMatch(read('$res/values/colors.xml'));
    expect(m, isNotNull, reason: 'values/colors.xml defines atlas_ground');
    return m!.group(1)!;
  }

  group('iOS launch screen', () {
    final storyboard = read('ios/Runner/Base.lproj/LaunchScreen.storyboard');

    test('its background is the app ground, in sRGB like Flutter draws it', () {
      final m = RegExp(
        r'<color key="backgroundColor" red="([\d.]+)" green="([\d.]+)" blue="([\d.]+)" alpha="1" colorSpace="custom" customColorSpace="sRGB"/>',
      ).firstMatch(storyboard);
      expect(
        m,
        isNotNull,
        reason:
            'an opaque sRGB background (a P3 or "calibrated" colour shifts)',
      );
      int channel(int i) => (double.parse(m!.group(i)!) * 255).round();
      final got = Color.fromARGB(
        0xFF,
        channel(1),
        channel(2),
        channel(3),
      ).toARGB32();
      expect(got, ground.toARGB32());
      // Rounded numbers (0.043 for 11/255) are one step off in 8-bit sRGB on some paths: use the exact ones.
      for (var i = 1; i <= 3; i++) {
        final v = double.parse(m!.group(i)!);
        expect((v * 255 - (v * 255).round()).abs(), lessThan(0.01));
      }
    });

    test('it shows the ground and nothing else: the globe forms in Flutter, so a native one would vanish and re-form', () {
      expect(storyboard, isNot(contains('<imageView')));
      expect(storyboard, isNot(contains('<label')));
    });
  });

  group('Android launch', () {
    test('values/colors.xml holds the app ground', () {
      expect(androidColor(groundXml()), ground);
      expect(groundXml().toLowerCase(), hex);
    });

    test(
      'no other file spells the ground out: they reference @color/atlas_ground',
      () {
        for (final f in [
          '$res/drawable/launch_background.xml',
          '$res/values/styles.xml',
          '$res/values-v31/styles.xml',
        ]) {
          final text = read(f);
          expect(
            RegExp(r'#[0-9a-fA-F]{6,8}').hasMatch(text),
            isFalse,
            reason: '$f hard-codes a colour',
          );
        }
      },
    );

    test('both themes paint the ground behind the first frame', () {
      final styles = read('$res/values/styles.xml');
      for (final theme in ['LaunchTheme', 'NormalTheme']) {
        final block = RegExp(
          '<style name="$theme"[^>]*>(.*?)</style>',
          dotAll: true,
        ).firstMatch(styles);
        expect(block, isNotNull, reason: theme);
        expect(
          block!.group(1),
          matches(
            RegExp(
              r'name="android:windowBackground">@(color/atlas_ground|drawable/launch_background)<',
            ),
          ),
          reason: '$theme must not fall back to the platform background',
        );
      }
      expect(
        read('$res/drawable/launch_background.xml'),
        contains('@color/atlas_ground'),
      );
    });

    test('Android 12+ system splash: the ground and no icon', () {
      final v31 = read('$res/values-v31/styles.xml');
      expect(
        v31,
        contains(
          'name="android:windowSplashScreenBackground">@color/atlas_ground<',
        ),
      );
      expect(
        v31,
        contains('name="android:windowSplashScreenAnimatedIcon">'),
        reason: 'unset, the system draws the launcher icon in a circle',
      );
      final icon = RegExp(r'windowSplashScreenAnimatedIcon">@drawable/(\w+)<')
          .firstMatch(v31);
      expect(icon, isNotNull);
      // Transparent: the globe forms in Flutter on the same ground.
      final drawable = read('$res/drawable/${icon!.group(1)}.xml');
      expect(
        RegExp(r'fillColor="(#\w+)"').allMatches(drawable).map((m) => m[1]),
        everyElement('#00000000'),
      );
      expect(
        drawable,
        contains('<path'),
        reason: 'with no path the system falls back to the launcher icon',
      );
    });
  });
}
