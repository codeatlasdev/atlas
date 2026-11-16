import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// Atlas Dev is a second app next to the store app, on the maintainer's own phone (flavors `dev` and
/// `prod`, docs/mobile.md "Dev and store apps"). These read the native project files (the only place
/// the identity lives) and fail when a change would let the two apps share state, or break the dev
/// app's name and id: the cost is his real pairings or a dev build that replaces the store app.
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
    test('the store app keeps its bundle id, name and icon in every mode', () {
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
      'the dev app is a different app: own bundle id, own icon, in every mode',
      () {
        final c = configs();
        for (final mode in ['Debug-dev', 'Release-dev', 'Profile-dev']) {
          final s = c[mode]!;
          expect(
            s,
            contains(
              'PRODUCT_BUNDLE_IDENTIFIER = br.com.codeatlas.atlas.mobile.dev;',
            ),
            reason: mode,
          );
          expect(s, contains('AppIcon-Dev'), reason: mode);
        }
      },
    );

    test('the dev app is named "Atlas Dev" and the store app "Atlas"', () {
      for (final f in ['Debug-dev', 'Release-dev', 'Profile-dev']) {
        expect(
          _read('ios/Flutter/$f.xcconfig'),
          contains('ATLAS_DISPLAY_NAME = Atlas Dev'),
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

    test('the dev scheme builds the dev configurations only', () {
      final s = _read(
        'ios/Runner.xcodeproj/xcshareddata/xcschemes/dev.xcscheme',
      );
      final used = RegExp(r'buildConfiguration = "([\w-]+)"')
          .allMatches(s)
          .map((m) => m[1])
          .toSet();
      expect(used, everyElement(endsWith('-dev')));
      // And the store scheme (`prod`, over the base configurations) never points at them.
      final store = _read(
        'ios/Runner.xcodeproj/xcshareddata/xcschemes/prod.xcscheme',
      );
      expect(store, isNot(contains('-dev"')));
    });

    test('the dev icons exist and differ from the store ones', () {
      final rel = File(
        'ios/Runner/Assets.xcassets/AppIcon.appiconset/Icon-App-1024x1024@1x.png',
      ).readAsBytesSync();
      final dev = File(
        'ios/Runner/Assets.xcassets/AppIcon-Dev.appiconset/Icon-App-1024x1024@1x.png',
      ).readAsBytesSync();
      expect(dev, isNot(rel));
    });

    // Bundle id isolation is the whole storage story: a shared keychain or app group would let
    // the dev app read, and so corrupt, the real app's pairings and phone key. Push needs
    // both an entitlement and a Firebase config; the dev app has neither.
    // firebase_* packages are allowed in pubspec because build.gradle.kts applies
    // google-services only when google-services.json is present (the dev app never has one).
    test('nothing is shared between the two apps and no push is set up', () {
      // The store app has push (Runner*.entitlements: aps-environment only); the dev app's
      // configurations sign with no entitlements at all, so it never gets push, a shared
      // keychain or an app group.
      for (final MapEntry(key: name, value: body) in configs().entries) {
        final dev = name.endsWith('-dev');
        expect(
          body.contains('CODE_SIGN_ENTITLEMENTS'),
          !dev,
          reason: '$name: entitlements are the store app\'s alone',
        );
      }
      for (final f in Directory('ios/Runner').listSync().whereType<File>()) {
        if (!f.path.endsWith('.entitlements')) continue;
        final ent = f.readAsStringSync();
        expect(ent, isNot(contains('keychain-access-groups')), reason: f.path);
        expect(ent, isNot(contains('application-groups')), reason: f.path);
      }
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
      for (final p in ['flutter_local_notifications']) {
        expect(
          pub,
          isNot(contains(p)),
          reason: 'push packages need a per-app config the dev app lacks',
        );
      }
      expect(File('ios/Runner/GoogleService-Info.plist').existsSync(), isFalse);
      expect(File('android/app/google-services.json').existsSync(), isFalse);
    });
  });

  group('Android', () {
    final gradle = _read('android/app/build.gradle.kts');

    test(
      'the dev flavor is the store id plus ".dev"; prod keeps the store id',
      () {
        expect(
          gradle,
          contains('applicationId = "br.com.codeatlas.atlas.mobile"'),
        );
        expect(gradle, contains('create("dev")'));
        expect(gradle, contains('applicationIdSuffix = ".dev"'));
        expect(gradle, contains('create("prod")'));
        // Only dev has a suffix: a second one would change the store id.
        expect(RegExp('applicationIdSuffix').allMatches(gradle).length, 1);
        expect(
          _read('pubspec.yaml'),
          contains('default-flavor: dev'),
          reason: 'plain flutter run must land on the dev app',
        );
      },
    );

    test('the store app is called Atlas and the dev app "Atlas Dev"', () {
      expect(
        _read('android/app/src/prod/res/values/strings.xml'),
        contains('<string name="app_name">Atlas</string>'),
      );
      expect(
        _read('android/app/src/dev/res/values/strings.xml'),
        contains('<string name="app_name">Atlas Dev</string>'),
      );
      expect(
        _read('android/app/src/main/AndroidManifest.xml'),
        contains('android:label="@string/app_name"'),
      );
    });

    test('the dev flavor has its own launcher icons, in every build type', () {
      for (final d in ['mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi']) {
        expect(
          File('android/app/src/dev/res/mipmap-$d/ic_launcher.png')
              .existsSync(),
          isTrue,
          reason: d,
        );
      }
    });
  });
}
