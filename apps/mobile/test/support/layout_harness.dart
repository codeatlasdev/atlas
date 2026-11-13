import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// A phone to lay a screen out on: the smallest we still support, a common
/// small Android, and a current iPhone.
typedef PhoneSize = ({String name, Size size});

const phoneSizes = <PhoneSize>[
  (name: '320x568', size: Size(320, 568)),
  (name: '360x640', size: Size(360, 640)),
  (name: '393x852', size: Size(393, 852)),
];

/// System text scale: the default and the largest the accessibility setting offers
/// before it stops being useful (Android caps at 2.0, iOS goes to ~3.1).
const textScales = <double>[1.0, 2.0];

/// Builds the screen under test; called once per (size, scale) combination, inside
/// the harness's `MaterialApp`, so it can use `Navigator`, `Theme` and `Scaffold`.
typedef ScreenBuilder = Widget Function(BuildContext context);

/// Puts something *above* the `MaterialApp` (a `ProviderScope`): sheets and dialogs
/// are built under the app's `Navigator`, which must see it too.
typedef AppWrapper = Widget Function(Widget app);

/// Runs [body] on a [size] screen at [scale] text scale with the surface, the text
/// scale and the safe areas (notch, home indicator) set, and undoes it afterwards.
///
/// [keyboard] raises a software keyboard of that height (`viewInsets.bottom`).
Future<void> withPhone(
  WidgetTester tester, {
  required Size size,
  required double scale,
  double keyboard = 0,
  required Future<void> Function() body,
}) async {
  final view = tester.view;
  view.physicalSize = size;
  view.devicePixelRatio = 1;
  // Notch on top, home indicator at the bottom: what a modern phone reserves.
  view.padding = const FakeViewPadding(top: 44, bottom: 34);
  view.viewPadding = const FakeViewPadding(top: 44, bottom: 34);
  view.viewInsets = FakeViewPadding(bottom: keyboard);
  tester.platformDispatcher.textScaleFactorTestValue = scale;
  try {
    await body();
  } finally {
    view.resetPhysicalSize();
    view.resetDevicePixelRatio();
    view.resetPadding();
    view.resetViewPadding();
    view.resetViewInsets();
    tester.platformDispatcher.clearTextScaleFactorTestValue();
  }
}

/// Wraps [screen] the way the app does (theme, Material, Navigator), under [wrap].
Widget appAround(Widget screen, [AppWrapper? wrap]) {
  final app = MaterialApp(theme: atlasTheme(), home: screen);
  return wrap == null ? app : wrap(app);
}

/// Pumps for [ms] in 50 ms steps. `pumpAndSettle` never returns on the screens
/// that animate forever (a running agent, a status pulse).
Future<void> settle(WidgetTester tester, [int ms = 600]) async {
  for (var i = 0; i < ms ~/ 50; i++) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

/// A framework error reduced to its headline and the widget that caused it (file and
/// line), so a failure says where to look instead of printing a render tree.
String _describe(FlutterErrorDetails d) {
  final head = d.exceptionAsString().split('\n').first.trim();
  String? where;
  final info = d.informationCollector?.call() ?? const [];
  for (final n in [
    ...?d.context == null ? null : [d.context!],
    ...info,
  ]) {
    final text = n.toStringDeep();
    final m = RegExp(r'file://\S*?/apps/mobile/(\S+)').firstMatch(text);
    if (m != null) {
      where = m.group(1);
      break;
    }
  }
  if (where == null) {
    final m = RegExp(r'file://\S*?/apps/mobile/(\S+)').firstMatch(d.toString());
    where = m?.group(1);
  }
  return where == null ? head : '$head  [$where]';
}

/// Lays the screen out on every phone size at every text scale and fails when any
/// of them reports a layout error (overflow, unbounded constraints, a render box
/// that was never laid out).
///
/// [prepare] runs after the first frame of each combination (open a sheet, scroll,
/// type a long hostname) and may pump; errors it causes are caught too.
/// [keyboard] also runs every combination with a software keyboard of that height.
/// [tapTargets] additionally asserts Material's 48 dp tap target on every tappable.
Future<void> expectNoLayoutProblems(
  WidgetTester tester,
  ScreenBuilder build, {
  AppWrapper? wrap,
  String label = 'screen',
  Future<void> Function(WidgetTester tester)? prepare,
  double keyboard = 0,
  bool tapTargets = true,
  Duration settleFor = const Duration(milliseconds: 600),
}) async {
  final problems = <String>[];
  for (final phone in phoneSizes) {
    for (final scale in textScales) {
      for (final kb in {0.0, keyboard}) {
        final where =
            '$label @ ${phone.name} x$scale${kb > 0 ? ' +keyboard' : ''}';
        // Every error, not just the first: the test binding keeps only one.
        final seen = <String>{};
        final previous = FlutterError.onError;
        FlutterError.onError = (d) => seen.add(_describe(d));
        try {
          await withPhone(
            tester,
            size: phone.size,
            scale: scale,
            keyboard: kb,
            body: () async {
              await tester.pumpWidget(appAround(Builder(builder: build), wrap));
              await settle(tester, settleFor.inMilliseconds);
              if (prepare != null) {
                try {
                  await prepare(tester);
                } on Object catch (e) {
                  // A layout error can abort the frame the step needed (no tree to tap).
                  seen.add('step failed: ${'$e'.split('\n').first}');
                }
                await settle(tester, 300);
              }
              if (tapTargets) {
                await _checkTapTargets(tester, where, seen);
              }
              // Tear down inside the phone, so a dispose-time error is attributed here.
              await tester.pumpWidget(const SizedBox());
              // Providers and timers owned by the screen are released on the next frames.
              await settle(tester, 3500);
            },
          );
        } finally {
          FlutterError.onError = previous;
        }
        for (final e in seen) {
          problems.add('$where: $e');
        }
      }
    }
  }
  if (problems.isNotEmpty) {
    fail('${problems.length} layout problem(s):\n\n${problems.join('\n\n')}');
  }
}

/// Tappable things that are not 48 dp are reported with where they are on screen.
Future<void> _checkTapTargets(
  WidgetTester tester,
  String where,
  Set<String> problems,
) async {
  final handle = tester.ensureSemantics();
  try {
    final result = await androidTapTargetGuideline.evaluate(tester);
    if (!result.passed) {
      // One line per node: `label: found 93x32`.
      for (final m in RegExp(
        r'label: "([^"]*)"[^)]*\): expected tap target size of at least Size\(48\.0, 48\.0\), but found Size\(([\d.]+), ([\d.]+)\)',
        dotAll: true,
      ).allMatches(result.reason ?? '')) {
        final label = m.group(1)!.replaceAll('\n', ' ');
        problems.add(
          'tap target "${label.length > 40 ? label.substring(0, 40) : label}" '
          '${double.parse(m.group(2)!).round()}x${double.parse(m.group(3)!).round()}',
        );
      }
    }
  } finally {
    handle.dispose();
  }
}

/// Scrolls the screen until [finder] is built and on screen (a lazy list does not build
/// what is far below the fold, so the target may not exist yet), then taps it.
/// Fails with the finder's description when it never shows up.
Future<void> scrollToAndTap(WidgetTester tester, Finder finder) async {
  if (finder.evaluate().isEmpty) {
    await tester.scrollUntilVisible(
      finder,
      200,
      scrollable: find.byType(Scrollable).first,
      maxScrolls: 40,
    );
  }
  await tester.ensureVisible(finder.first);
  await settle(tester, 200);
  await tester.tap(finder.first, warnIfMissed: false);
  await settle(tester, 700);
}
