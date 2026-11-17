import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_provider.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/settings/settings_screen.dart';

import 'support/fake_permissions.dart';

// Settings → This phone: the real state of each permission, with the action that fits it. The only place a
// person who said "Not now" (or "Don't Allow") finds the way back.

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<List<MacDevice>> loadDevices() async => const [];
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

const _n = AtlasPermission.notifications;
const _c = AtlasPermission.camera;
const _l = AtlasPermission.localNetwork;

Future<FakePermissions> _pump(
  WidgetTester t,
  Map<AtlasPermission, PermissionState> states, {
  String? push,
  Map<AtlasPermission, PermissionState> answers = const {},
}) async {
  final perms = FakePermissions(
    states: {
      // Not on every phone: a build without push, no camera, not iOS.
      _n: PermissionState.unavailable,
      _c: PermissionState.unavailable,
      _l: PermissionState.unavailable,
      ...states,
    },
    answers: answers,
  );
  await t.binding.setSurfaceSize(const Size(393, 1200));
  addTearDown(() => t.binding.setSurfaceSize(null));
  await t.pumpWidget(const SizedBox()); // a fresh app per call
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage()),
        permissionsProvider.overrideWithValue(perms),
        pushRegistrationProvider.overrideWith((ref) async => push),
      ],
      child: MaterialApp(theme: atlasTheme(), home: const SettingsScreen()),
    ),
  );
  await t.pump();
  await t.pump(const Duration(milliseconds: 600));
  return perms;
}

void main() {
  group('notifications', () {
    testWidgets('not asked: says so, and Turn on explains before it asks', (
      t,
    ) async {
      final perms = await _pump(t, {_n: PermissionState.notAsked});
      expect(find.text('This phone'), findsOneWidget);
      expect(find.text('Notifications'), findsOneWidget);
      expect(find.textContaining('Not asked yet'), findsOneWidget);
      expect(perms.requested, isEmpty, reason: 'nothing at launch');

      await t.tap(find.text('Turn on'));
      await t.pumpAndSettle();
      expect(find.text('Get notified when you are away'), findsOneWidget);
      expect(perms.requested, isEmpty);

      await t.tap(find.text('Continue'));
      await t.pumpAndSettle();
      expect(perms.requested, [_n]);
      expect(find.text('Turn on'), findsNothing, reason: 'now on');
      expect(find.text('On'), findsOneWidget);
    });

    testWidgets('Not now leaves it as it was', (t) async {
      final perms = await _pump(t, {_n: PermissionState.notAsked});
      await t.tap(find.text('Turn on'));
      await t.pumpAndSettle();
      await t.tap(find.text('Not now'));
      await t.pumpAndSettle();
      expect(perms.requested, isEmpty);
      expect(find.text('Turn on'), findsOneWidget);
    });

    testWidgets('off in the system Settings: Open Settings', (t) async {
      final perms = await _pump(t, {_n: PermissionState.blocked});
      expect(find.textContaining('Off'), findsWidgets);
      await t.tap(find.text('Open Settings'));
      expect(perms.settingsOpened, 1);
      expect(perms.requested, isEmpty);
    });

    testWidgets(
      'on: says On, or the reason the computers did not take the registration',
      (t) async {
        await _pump(t, {_n: PermissionState.granted});
        expect(find.text('On'), findsOneWidget);
        expect(find.text('Turn on'), findsNothing);

        await _pump(t, {
          _n: PermissionState.granted,
        }, push: 'Notifications are off: the push relay is down (HTTP 503)');
        expect(
          find.text('Notifications are off: the push relay is down (HTTP 503)'),
          findsOneWidget,
        );
      },
    );

    testWidgets('not in this build (no Firebase config): no row at all', (
      t,
    ) async {
      await _pump(t, {});
      expect(find.text('Notifications'), findsNothing);
      expect(find.text('This phone'), findsNothing);
    });

    testWidgets(
      'turned on in the system Settings: the row follows when the app returns',
      (t) async {
        final perms = await _pump(t, {_n: PermissionState.blocked});
        expect(find.text('Open Settings'), findsOneWidget);

        perms.states[_n] = PermissionState.granted;
        t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
        t.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
        await t.pumpAndSettle();
        expect(find.text('Open Settings'), findsNothing);
        expect(find.text('On'), findsOneWidget);
      },
    );
  });

  group('camera', () {
    testWidgets('each state has its words and its action', (t) async {
      final perms = await _pump(t, {_c: PermissionState.notAsked});
      expect(find.text('Camera'), findsOneWidget);
      expect(find.textContaining('Not asked yet'), findsOneWidget);
      await t.tap(find.text('Allow'));
      await t.pumpAndSettle();
      expect(find.text('Allow the camera'), findsOneWidget);
      expect(perms.requested, isEmpty);
      await t.tap(find.text('Continue'));
      await t.pumpAndSettle();
      expect(perms.requested, [_c]);

      final off = await _pump(t, {_c: PermissionState.blocked});
      expect(
        find.textContaining('cannot scan the pairing code'),
        findsOneWidget,
      );
      await t.tap(find.text('Open Settings'));
      expect(off.settingsOpened, 1);

      await _pump(t, {_c: PermissionState.restricted});
      expect(find.textContaining('Restricted'), findsOneWidget);
      expect(find.text('Open Settings'), findsNothing);
    });
  });

  group('local network (iOS)', () {
    testWidgets(
      'not asked: Atlas uses the relay until allowed; Allow explains first',
      (t) async {
        final perms = await _pump(t, {_l: PermissionState.notAsked});
        expect(find.text('Local network'), findsOneWidget);
        expect(find.textContaining('relay'), findsOneWidget);
        await t.tap(find.text('Allow'));
        await t.pumpAndSettle();
        expect(find.text('Find your computer on Wi-Fi'), findsOneWidget);
        expect(perms.requested, isEmpty);
        await t.tap(find.text('Continue'));
        await t.pumpAndSettle();
        expect(perms.requested, [_l]);
      },
    );

    testWidgets(
      'asked: how to change it, with Open Settings (iOS cannot say what was chosen)',
      (t) async {
        final perms = await _pump(t, {_l: PermissionState.granted});
        expect(find.textContaining('Don\'t Allow'), findsOneWidget);
        await t.tap(find.text('Open Settings'));
        expect(perms.settingsOpened, 1);
      },
    );

    testWidgets('not iOS: no row', (t) async {
      await _pump(t, {_c: PermissionState.granted});
      expect(find.text('Local network'), findsNothing);
    });
  });
}
