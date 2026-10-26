import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// Atlas Lab is a second app next to the real one, on the maintainer's own phone. These read the
/// native project files (the only place the identity lives) and fail when a change would let the two
/// apps share state, or break the lab's name and id: the cost is his real pairings or a lab that
/// replaces the real app on install.
String _read(String path) => File(path).readAsStringSync();

void main() {
  final pbx = _read('ios/Runner.xcodeproj/project.pbxproj');

  // Every build configuration of the Runner target, with its settings.
  Map<String, String> configs() {
    final out = <String, String>{};
    final re = RegExp(
      r'/\* ([\w-]+) \*/ = \{\s*isa = XCBuildConfiguration;(.*?)\n\t\t\};',
      dotAll: true,
    );
    for (final m in re.allMatches(pbx)) {
      if (m[2]!.contains('INFOPLIST_FILE = Runner/Info.plist')) {
        out[m[1]!] = m[2]!;
      }
    }
    return out;
  }

  group('iOS', () {
    test('the real app keeps its bundle id, name and icon in every mode', () {
      final c = configs();
      for (final mode in ['Debug', 'Release', 'Profile']) {
        final s = c[mode]!;
        expect(
          s,
          contains(
            'PRODUCT_BUNDLE_IDENTIFIER = br.com.codeatlas.atlas.mobile;',
          ),
          reason: mode,
        );
        expect(
          s,
          contains('ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon;'),
          reason: mode,
        );
      }
    });

    test(
      'the lab is a different app: own bundle id, own icon, in every mode',
      () {
        final c = configs();
        for (final mode in ['Debug-lab', 'Release-lab', 'Profile-lab']) {
          final s = c[mode]!;
          expect(
            s,
            contains(
              'PRODUCT_BUNDLE_IDENTIFIER = br.com.codeatlas.atlas.mobile.lab;',
            ),
            reason: mode,
          );
          expect(s, contains('AppIcon-Lab'), reason: mode);
        }
      },
    );

    test('the lab is named "Atlas Lab" and the real app "Atlas"', () {
      for (final f in ['Debug-lab', 'Release-lab', 'Profile-lab']) {
        expect(
          _read('ios/Flutter/$f.xcconfig'),
          contains('ATLAS_DISPLAY_NAME = Atlas Lab'),
        );
      }
      for (final f in ['Debug', 'Release']) {
        expect(
          _read('ios/Flutter/$f.xcconfig'),
          contains('ATLAS_DISPLAY_NAME = Atlas\n'),
        );
      }
      expect(
        _read('ios/Runner/Info.plist'),
        contains(r'$(ATLAS_DISPLAY_NAME)'),
      );
    });

    test('the lab scheme builds the lab configurations only', () {
      final s = _read(
        'ios/Runner.xcodeproj/xcshareddata/xcschemes/lab.xcscheme',
      );
      final used = RegExp(r'buildConfiguration = "([\w-]+)"')
          .allMatches(s)
          .map((m) => m[1])
          .toSet();
      expect(used, everyElement(endsWith('-lab')));
      // And the real scheme never points at them.
      final real = _read(
        'ios/Runner.xcodeproj/xcshareddata/xcschemes/Runner.xcscheme',
      );
      expect(real, isNot(contains('-lab"')));
    });

    test('the lab icons exist and differ from the release ones', () {
      final rel = File(
        'ios/Runner/Assets.xcassets/AppIcon.appiconset/Icon-App-1024x1024@1x.png',
      ).readAsBytesSync();
      final lab = File(
        'ios/Runner/Assets.xcassets/AppIcon-Lab.appiconset/Icon-App-1024x1024@1x.png',
      ).readAsBytesSync();
      expect(lab, isNot(rel));
    });

    // Bundle id isolation is the whole storage story: a shared keychain or app group would let
    // the lab read, and so corrupt, the real app's pairings and phone key. Push needs
    // an entitlement; the lab has no Firebase/APNs identity, so none may appear unnoticed.
    test('nothing is shared between the two apps and no push is set up', () {
      expect(
        Directory('ios/Runner').listSync().map((e) => e.path),
        isNot(contains(endsWith('.entitlements'))),
        reason: 'an entitlements file (keychain/app group/push) would also apply to the lab',
      );
      for (final key in [
        'keychain-access-groups',
        'application-groups',
        'aps-environment',
      ]) {
        expect(pbx, isNot(contains(key)));
      }
      final info = _read('ios/Runner/Info.plist');
      expect(info, isNot(contains('remote-notification')));
      final pub = _read('pubspec.yaml');
      for (final p in ['firebase_', 'flutter_local_notifications']) {
        expect(
          pub,
          isNot(contains(p)),
          reason: 'push packages need a per-app config the lab lacks',
        );
      }
      expect(File('ios/Runner/GoogleService-Info.plist').existsSync(), isFalse);
      expect(File('android/app/google-services.json').existsSync(), isFalse);
    });
  });

  group('Android', () {
    final gradle = _read('android/app/build.gradle.kts');

    test('the lab id is the real id plus ".lab", only when asked for', () {
      expect(
        gradle,
        contains('val atlasLab = project.findProperty("atlas.lab") == "1"'),
      );
      expect(
        gradle,
        contains(
          'applicationId = "br.com.codeatlas.atlas.mobile" + if (atlasLab) ".lab" else ""',
        ),
      );
    });

    test('the real app is still called Atlas and the lab "Atlas Lab"', () {
      expect(
        gradle,
        contains(
          'resValue("string", "app_name", if (atlasLab) "Atlas Lab" else "Atlas")',
        ),
      );
      expect(
        _read('android/app/src/main/AndroidManifest.xml'),
        contains('android:label="@string/app_name"'),
      );
    });

    test('the lab has its own launcher icons', () {
      for (final d in ['mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi']) {
        expect(
          File('android/app/src/lab/res/mipmap-$d/ic_launcher.png')
              .existsSync(),
          isTrue,
          reason: d,
        );
      }
    });

    test(
      'no flavor was added: the release lanes build the same outputs as before',
      () {
        expect(gradle, isNot(contains('productFlavors')));
        expect(gradle, isNot(contains('flavorDimensions')));
      },
    );
  });
}
