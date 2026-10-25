import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/lab/lab.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

Future<void> _show(WidgetTester t, LabConfig lab, {Widget? child}) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  await t.pumpWidget(
    MaterialApp(
      theme: atlasTheme(),
      builder: (context, app) => LabFrame(lab: lab, child: app!),
      home: child ?? const Scaffold(body: Center(child: Text('the app'))),
    ),
  );
  await t.pump();
}

void main() {
  group('LabConfig', () {
    // Guards the real app: the lab is on only when the build said so, never by accident.
    test('is off unless ATLAS_LAB is set', () {
      expect(const LabConfig(enabled: false, sha: '').enabled, isFalse);
      expect(
        LabConfig.fromEnvironment().enabled,
        isFalse,
        reason: 'flutter test passes no --dart-define=ATLAS_LAB=1',
      );
    });

    test('the banner names the lab and the commit it was built from', () {
      expect(
        const LabConfig(enabled: true, sha: 'a1b2c3d').banner,
        'LAB · a1b2c3d',
      );
    });

    // `bun run mobile:lab` passes the sha, but a build outside it must still say it is a lab.
    test('a lab with no sha still reads as a lab, not "LAB · "', () {
      expect(const LabConfig(enabled: true, sha: '').banner, 'LAB');
      expect(const LabConfig(enabled: true, sha: '  ').banner, 'LAB');
    });

    // The sha comes from a shell: keep a pasted newline or a long ref from breaking the strip.
    test('the sha is trimmed and clipped to a short form', () {
      expect(
        const LabConfig(
          enabled: true,
          sha: ' 0123456789abcdef0123456789abcdef01234567\n',
        ).banner,
        'LAB · 0123456789ab',
      );
    });
  });

  group('LabFrame', () {
    testWidgets('the real app shows no banner and the app fills the screen', (
      t,
    ) async {
      await _show(t, const LabConfig(enabled: false, sha: ''));
      expect(find.textContaining('LAB'), findsNothing);
      expect(find.text('the app'), findsOneWidget);
      expect(
        t.getRect(find.byType(Scaffold)).height,
        852,
        reason: 'the release app must not lose a pixel to the lab strip',
      );
    });

    testWidgets('the lab shows "LAB · sha" above the app', (t) async {
      await _show(t, const LabConfig(enabled: true, sha: 'a1b2c3d'));
      expect(find.text('LAB · a1b2c3d'), findsOneWidget);
      expect(find.text('the app'), findsOneWidget);
      final strip = t.getRect(find.text('LAB · a1b2c3d'));
      final app = t.getRect(find.text('the app'));
      expect(strip.bottom, lessThan(app.top), reason: 'the strip is on top');
    });

    // A slim strip, not a toolbar: it must not eat the screen (and the app below it
    // keeps all the rest, status bar aside).
    testWidgets('the strip is slim and the app gets the rest of the height', (
      t,
    ) async {
      await _show(t, const LabConfig(enabled: true, sha: 'a1b2c3d'));
      final strip = t.getRect(find.byKey(const ValueKey('lab-banner')));
      expect(strip.height, lessThanOrEqualTo(28));
      final scaffold = t.getRect(find.byType(Scaffold));
      expect(scaffold.height, closeTo(852 - strip.height, 0.5));
    });

    // The strip must not swallow taps meant for the screen (it sits above, outside it).
    testWidgets('the app below the strip stays interactive', (t) async {
      var taps = 0;
      await _show(
        t,
        const LabConfig(enabled: true, sha: 'a1b2c3d'),
        child: Scaffold(
          body: Center(
            child: TextButton(
              onPressed: () => taps++,
              child: const Text('tap me'),
            ),
          ),
        ),
      );
      await t.tap(find.text('tap me'));
      expect(taps, 1);
    });
  });
}
