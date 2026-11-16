import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:permission_handler/permission_handler.dart' as ph;

import 'package:atlas_mobile/core/permissions/permission_ui.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/permissions/system_permissions.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

import 'support/fake_permissions.dart';

// The one way the app asks for a permission: explain first, show the system prompt only after "Continue".
// The platform is faked; the sheet, the flow and the states are the real ones.

PermissionState? _result;

Future<FakePermissions> _pump(WidgetTester t, FakePermissions perms) async {
  _result = null;
  await t.pumpWidget(
    ProviderScope(
      overrides: [permissionsProvider.overrideWithValue(perms)],
      child: MaterialApp(
        theme: atlasTheme(),
        home: Consumer(
          builder: (context, ref, _) => Scaffold(
            body: Center(
              child: FilledButton(
                onPressed: () async => _result = await askPermission(
                  context,
                  ref,
                  AtlasPermission.camera,
                ),
                child: const Text('Ask'),
              ),
            ),
          ),
        ),
      ),
    ),
  );
  return perms;
}

void main() {
  testWidgets('already allowed: nothing shown, nothing requested', (t) async {
    final perms = await _pump(t, FakePermissions());
    await t.tap(find.text('Ask'));
    await t.pumpAndSettle();

    expect(find.text('Continue'), findsNothing);
    expect(perms.requested, isEmpty);
    expect(_result, PermissionState.granted);
  });

  testWidgets(
    'not asked: the explanation comes first, the prompt only after Continue',
    (t) async {
      final perms = await _pump(
        t,
        FakePermissions(
          states: {AtlasPermission.camera: PermissionState.notAsked},
        ),
      );
      await t.tap(find.text('Ask'));
      await t.pumpAndSettle();

      expect(find.text('Allow the camera'), findsOneWidget);
      expect(find.textContaining('Nothing is recorded'), findsOneWidget);
      expect(
        perms.requested,
        isEmpty,
        reason: 'no system prompt behind the explanation',
      );

      await t.tap(find.text('Continue'));
      await t.pumpAndSettle();

      expect(perms.requested, [AtlasPermission.camera]);
      expect(_result, PermissionState.granted);
      expect(find.text('Allow the camera'), findsNothing);
    },
  );

  testWidgets('Not now: nothing is requested and nothing is granted', (
    t,
  ) async {
    final perms = await _pump(
      t,
      FakePermissions(
        states: {AtlasPermission.camera: PermissionState.notAsked},
      ),
    );
    await t.tap(find.text('Ask'));
    await t.pumpAndSettle();
    await t.tap(find.text('Not now'));
    await t.pumpAndSettle();

    expect(perms.requested, isEmpty);
    expect(_result, PermissionState.notAsked);
  });

  testWidgets(
    'refused in the system prompt: the caller gets the blocked state',
    (t) async {
      final perms = await _pump(
        t,
        FakePermissions(
          states: {AtlasPermission.camera: PermissionState.notAsked},
          answers: {AtlasPermission.camera: PermissionState.blocked},
        ),
      );
      await t.tap(find.text('Ask'));
      await t.pumpAndSettle();
      await t.tap(find.text('Continue'));
      await t.pumpAndSettle();

      expect(perms.requested, [AtlasPermission.camera]);
      expect(_result, PermissionState.blocked);
    },
  );

  testWidgets(
    'blocked, restricted, unavailable: no sheet and no prompt (the system would not show one)',
    (t) async {
      for (final state in [
        PermissionState.blocked,
        PermissionState.restricted,
        PermissionState.unavailable,
      ]) {
        final perms = await _pump(
          t,
          FakePermissions(states: {AtlasPermission.camera: state}),
        );
        await t.tap(find.text('Ask'));
        await t.pumpAndSettle();

        expect(find.text('Continue'), findsNothing, reason: '$state');
        expect(perms.requested, isEmpty, reason: '$state');
        expect(_result, state);
      }
    },
  );

  group('states from the platform plugins', () {
    test(
      'camera: iOS never asked is denied, a refusal is permanently denied',
      () {
        expect(
          fromHandler(ph.PermissionStatus.denied),
          PermissionState.notAsked,
        );
        expect(
          fromHandler(ph.PermissionStatus.permanentlyDenied),
          PermissionState.blocked,
        );
        expect(
          fromHandler(ph.PermissionStatus.restricted),
          PermissionState.restricted,
        );
        expect(
          fromHandler(ph.PermissionStatus.granted),
          PermissionState.granted,
        );
        expect(
          fromHandler(ph.PermissionStatus.limited),
          PermissionState.granted,
        );
      },
    );

    test(
      'notifications: iOS denied is final, Android denied can be asked again',
      () {
        expect(
          fromAuthorization(AuthorizationStatus.denied, ios: true),
          PermissionState.blocked,
        );
        expect(
          fromAuthorization(AuthorizationStatus.denied, ios: false),
          PermissionState.notAsked,
        );
        expect(
          fromAuthorization(AuthorizationStatus.deniedPermanently, ios: false),
          PermissionState.blocked,
        );
        expect(
          fromAuthorization(AuthorizationStatus.notDetermined, ios: true),
          PermissionState.notAsked,
        );
        expect(
          fromAuthorization(AuthorizationStatus.provisional, ios: true),
          PermissionState.granted,
        );
      },
    );
  });

  testWidgets('the off card: Settings only where Settings can help', (t) async {
    Future<FakePermissions> show(PermissionState state) async {
      final perms = FakePermissions();
      await t.pumpWidget(
        ProviderScope(
          overrides: [permissionsProvider.overrideWithValue(perms)],
          child: MaterialApp(
            theme: atlasTheme(),
            home: Scaffold(
              body: PermissionOffCard(
                permission: AtlasPermission.camera,
                state: state,
              ),
            ),
          ),
        ),
      );
      return perms;
    }

    final blocked = await show(PermissionState.blocked);
    expect(find.text('Camera access is off'), findsOneWidget);
    await t.tap(find.text('Open Settings'));
    expect(blocked.settingsOpened, 1);

    await show(PermissionState.restricted);
    expect(find.text('Restricted on this phone'), findsOneWidget);
    expect(find.text('Open Settings'), findsNothing);

    await show(PermissionState.unavailable);
    expect(find.text('Open Settings'), findsNothing);
  });
}
