import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/permissions/permission_ui.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/main.dart';

import 'support/demo_harness.dart';
import 'support/fake_permissions.dart';

// Regression (screenshot 2026-11-20, Settings > Local network > Allow): the "Find your computer on Wi-Fi" sheet
// opened under the floating tab bar, which covered its Continue button and stayed tappable over the scrim. Sheets
// a tab opens land on that tab's navigator, below the bar the shell draws. Through the REAL shell: while any popup
// is open on a tab the bar is gone (not tappable, not visible), and it comes back when the popup closes.

const _mac = MacDevice(nodeId: 'mac-1', name: 'Mac');

Future<void> _launch(WidgetTester t, FakePermissions perms) async {
  await t.binding.setSurfaceSize(const Size(393, 852));
  final router = buildRouter();
  addTearDown(router.dispose);
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage([_mac])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => demoTransport(),
        ),
        onboardingSeenProvider.overrideWith((_) => true),
        permissionsProvider.overrideWithValue(perms),
      ],
      child: MaterialApp.router(theme: atlasTheme(), routerConfig: router),
    ),
  );
  await settle(t);
}

Finder get _bar => find.byType(FloatingTabBar);

void main() {
  testWidgets(
    'the Local network explanation from Settings is never under the bar',
    (t) async {
      final perms = FakePermissions(
        states: {AtlasPermission.localNetwork: PermissionState.notAsked},
      );
      await _launch(t, perms);
      await t.tap(find.byIcon(Icons.settings_rounded));
      await settle(t);

      final row = find.text('Local network');
      await t.scrollUntilVisible(
        row,
        200,
        scrollable: find.byType(Scrollable).first,
      );
      await t.tap(
        find.descendant(
          of: find.ancestor(of: row, matching: find.byType(Row)).first,
          matching: find.widgetWithText(TextButton, 'Allow'),
        ),
      );
      await settle(t);

      expect(find.byType(PermissionExplainer), findsOneWidget);
      expect(
        _bar.hitTestable(),
        findsNothing,
        reason: 'the bar sits on the sheet',
      );
      // Every button of the sheet is reachable: nothing covers it.
      final buttons = find.descendant(
        of: find.byType(PermissionExplainer),
        matching: find.byWidgetPredicate((w) => w is ButtonStyleButton),
      );
      expect(buttons, findsWidgets);
      for (final b in buttons.evaluate()) {
        expect(
          find.byWidget(b.widget).hitTestable(),
          findsOneWidget,
          reason: '${b.widget} is covered',
        );
      }
    },
  );

  testWidgets('any sheet or dialog a tab opens hides the bar until it closes', (
    t,
  ) async {
    await _launch(t, FakePermissions());
    expect(_bar.hitTestable(), findsOneWidget);
    final home = t.element(find.byType(HomeScreen));

    // A tab screen's default: the nearest navigator, the tab's own (below the bar).
    showModalBottomSheet<void>(
      context: home,
      builder: (_) => const SizedBox(height: 300, child: Text('sheet')),
    );
    await settle(t);
    expect(find.text('sheet'), findsOneWidget);
    expect(_bar.hitTestable(), findsNothing);

    Navigator.of(t.element(find.text('sheet'))).pop();
    await settle(t);
    expect(_bar.hitTestable(), findsOneWidget, reason: 'the bar came back');

    showDialog<void>(
      context: home,
      useRootNavigator: false,
      builder: (_) => const AlertDialog(content: Text('dialog')),
    );
    await settle(t);
    expect(_bar.hitTestable(), findsNothing);
    Navigator.of(t.element(find.text('dialog'))).pop();
    await settle(t);
    expect(_bar.hitTestable(), findsOneWidget);
  });
}
