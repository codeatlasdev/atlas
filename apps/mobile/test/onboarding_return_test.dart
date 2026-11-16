// Regression (iPhone, 2026-11-16): Skip, then "Scan pairing code", then back: the onboarding's last page
// came back empty (dots and button only, no illustration, no "Pair your computer" steps). Each page fades
// by its distance from the current page. When the PageView's size changed (a frame with no size: the app
// leaving the foreground, a system prompt), the scroll moved back to the same page without notifying, the
// pages had been laid out against page 0, the last page stayed at opacity 0, and the layout threw on a
// negative width. The zero-size test below is the one that failed before the fix; the other two paths
// pass in the test binding and stay as guards.
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/home/onboarding.dart';
import 'package:atlas_mobile/main.dart';

import 'support/demo_harness.dart';
import 'support/fake_permissions.dart';

/// How opaque [f] is on screen: the product of every Opacity above it.
double _opacity(WidgetTester t, Finder f) {
  var o = 1.0;
  t.element(f).visitAncestorElements((e) {
    final w = e.widget;
    if (w is Opacity) o *= w.opacity;
    if (w is FadeTransition) o *= w.opacity.value;
    return true;
  });
  return o;
}

Future<GoRouter> _launch(WidgetTester t) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  final router = buildRouter();
  addTearDown(router.dispose);
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage(const [])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => demoTransport(),
        ),
        permissionsProvider.overrideWithValue(FakePermissions()),
      ],
      child: MaterialApp.router(theme: atlasTheme(), routerConfig: router),
    ),
  );
  await settle(t);
  return router;
}

void main() {
  testWidgets('the last page is whole again after the pairing screen', (
    t,
  ) async {
    final router = await _launch(t);
    await t.tap(find.text('Skip'));
    await settle(t);
    final title = find.text('Pair your computer');
    expect(_opacity(t, title), greaterThan(.99), reason: 'after Skip');

    await t.tap(find.widgetWithText(FilledButton, 'Scan pairing code'));
    await settle(t);
    router.pop();
    await settle(t);

    expect(find.byType(Onboarding), findsOneWidget);
    expect(
      _opacity(t, title),
      greaterThan(.99),
      reason: 'the last page came back empty after the pairing screen',
    );
  });

  testWidgets(
    'the last page is whole again after the app went to the background',
    (t) async {
      await _launch(t);
      await t.tap(find.text('Skip'));
      await settle(t);
      final title = find.text('Pair your computer');
      for (final s in [
        AppLifecycleState.inactive,
        AppLifecycleState.hidden,
        AppLifecycleState.paused,
      ]) {
        t.binding.handleAppLifecycleStateChanged(s);
        await t.pump();
      }
      for (final s in [
        AppLifecycleState.hidden,
        AppLifecycleState.inactive,
        AppLifecycleState.resumed,
      ]) {
        t.binding.handleAppLifecycleStateChanged(s);
        await t.pump();
      }
      await settle(t);
      expect(
        _opacity(t, title),
        greaterThan(.99),
        reason: 'after background and back',
      );
    },
  );

  testWidgets(
    'the last page is whole again after the view had no size for a moment',
    (t) async {
      await _launch(t);
      await t.tap(find.text('Skip'));
      await settle(t);
      final title = find.text('Pair your computer');
      await t.binding.setSurfaceSize(Size.zero);
      await t.pump();
      await t.binding.setSurfaceSize(const Size(393, 852));
      await settle(t);
      expect(
        _opacity(t, title),
        greaterThan(.99),
        reason: 'after a zero-size frame',
      );
    },
  );
}
