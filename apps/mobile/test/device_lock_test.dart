import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:local_auth/local_auth.dart';

import 'package:atlas_mobile/core/auth/device_auth.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';

import 'support/demo_harness.dart';
import 'support/fake_permissions.dart';

// Typing into a Mac's terminal from the phone is behind Face ID (or the passcode). The explanation comes
// before the system's Face ID prompt, and a phone that cannot unlock says so and why, never nothing.

class _Rig {
  _Rig(this.perms, {this.auth});
  final FakePermissions perms;
  final Future<bool> Function()? auth;
  var asked = 0;
  String? reason;
}

Future<_Rig> _open(
  WidgetTester t,
  FakePermissions perms, {
  Future<bool> Function()? auth,
}) async {
  final rig = _Rig(perms, auth: auth);
  await t.pumpWidget(
    demoWrap(
      const TerminalScreen(device: demoDevice, pty: 'p', title: 'zsh'),
      [demoDevice],
      overrides: [
        permissionsProvider.overrideWithValue(perms),
        deviceAuthProvider.overrideWithValue((reason) async {
          rig.asked++;
          rig.reason = reason;
          return auth == null ? true : auth();
        }),
      ],
    ),
  );
  await settle(t);
  return rig;
}

Future<void> _tapToType(WidgetTester t) async {
  await t.tap(find.text('Tap to type'));
  await settle(t);
}

final _unlocked = find.byTooltip('Stop typing');

void main() {
  testWidgets('granted: straight to the lock, and typing unlocks', (t) async {
    final rig = await _open(t, FakePermissions());
    await _tapToType(t);
    expect(rig.asked, 1);
    expect(rig.reason, contains('terminal'));
    expect(_unlocked, findsOneWidget);
    expect(find.text('Continue'), findsNothing);
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });

  testWidgets('first time on Face ID: explained before the lock is asked', (
    t,
  ) async {
    final perms = FakePermissions(
      states: {AtlasPermission.faceId: PermissionState.notAsked},
    );
    final rig = await _open(t, perms);
    await _tapToType(t);

    expect(find.text('Confirm it is you'), findsOneWidget);
    expect(find.textContaining('never sees your face'), findsOneWidget);
    expect(rig.asked, 0, reason: 'no Face ID prompt behind the explanation');

    await t.tap(find.text('Continue'));
    await settle(t);
    expect(rig.asked, 1);
    expect(_unlocked, findsOneWidget);
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });

  testWidgets('Not now: nothing is asked and the terminal stays read-only', (
    t,
  ) async {
    final rig = await _open(
      t,
      FakePermissions(
        states: {AtlasPermission.faceId: PermissionState.notAsked},
      ),
    );
    await _tapToType(t);
    await t.tap(find.text('Not now'));
    await settle(t);
    expect(rig.asked, 0);
    expect(_unlocked, findsNothing);
    expect(find.text('Tap to type'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });

  testWidgets(
    'a refused or cancelled lock keeps the terminal read-only, without an error',
    (t) async {
      final rig = await _open(t, FakePermissions(), auth: () async => false);
      await _tapToType(t);
      expect(rig.asked, 1);
      expect(_unlocked, findsNothing);
      expect(find.byType(SnackBar), findsNothing);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    },
  );

  testWidgets(
    'no passcode and no biometrics: says so and what to do; nothing is asked',
    (t) async {
      final rig = await _open(
        t,
        FakePermissions(
          states: {AtlasPermission.faceId: PermissionState.unavailable},
        ),
      );
      await _tapToType(t);
      expect(find.text('This phone has no lock set up'), findsOneWidget);
      expect(find.textContaining('Set a passcode'), findsOneWidget);
      expect(rig.asked, 0);
      expect(_unlocked, findsNothing);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    },
  );

  testWidgets(
    'the lock reports it cannot be used now (too many tries): the message shows',
    (t) async {
      await _open(
        t,
        FakePermissions(),
        auth: () async => throw const DeviceAuthUnavailable(
          'Too many attempts. Try again in a moment.',
        ),
      );
      await _tapToType(t);
      expect(
        find.text('Too many attempts. Try again in a moment.'),
        findsOneWidget,
      );
      expect(_unlocked, findsNothing);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    },
  );

  group('what the lock plugin reports', () {
    String? say(LocalAuthExceptionCode code) =>
        deviceAuthFailure(LocalAuthException(code: code))?.message;

    test('no lock set up and lockouts become messages with a way forward', () {
      expect(
        say(LocalAuthExceptionCode.noCredentialsSet),
        contains('passcode'),
      );
      expect(
        say(LocalAuthExceptionCode.biometricLockout),
        contains('passcode'),
      );
      expect(
        say(LocalAuthExceptionCode.temporaryLockout),
        contains('Try again'),
      );
      expect(
        say(LocalAuthExceptionCode.noBiometricsEnrolled),
        contains('passcode'),
      );
    });

    test('a person cancelling is not an error', () {
      expect(say(LocalAuthExceptionCode.userCanceled), isNull);
      expect(say(LocalAuthExceptionCode.systemCanceled), isNull);
    });
  });
}
