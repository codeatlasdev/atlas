import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';
import 'package:atlas_mobile/features/crew/chat/composer.dart';

import 'support/fake_permissions.dart';

// A photo for a crewmate: the camera is the only photo source with a permission. It is explained before the
// system prompt, and when it is off the person can still choose from the library (which needs none).

final _png = base64.decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
);

class _Picker implements ImagePickerPort {
  var cameraCalls = 0, galleryCalls = 0;
  @override
  Future<List<PickedImage>> camera() async {
    cameraCalls++;
    return [PickedImage(name: 'shot.png', bytes: _png)];
  }

  @override
  Future<List<PickedImage>> gallery() async {
    galleryCalls++;
    return [PickedImage(name: 'lib.png', bytes: _png)];
  }
}

Future<_Picker> _pump(WidgetTester t, FakePermissions perms) async {
  final picker = _Picker();
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        imagePickerProvider.overrideWithValue(picker),
        permissionsProvider.overrideWithValue(perms),
      ],
      child: MaterialApp(
        theme: atlasTheme(),
        home: Scaffold(
          body: Align(
            alignment: Alignment.bottomCenter,
            child: ChatComposer(
              controller: TextEditingController(),
              sending: false,
              online: true,
              queued: 0,
              onSend: (_, _) async => true,
            ),
          ),
        ),
      ),
    ),
  );
  await t.tap(find.bySemanticsLabel('Attach image'));
  await t.pumpAndSettle();
  return picker;
}

Future<void> _takePhoto(WidgetTester t) async {
  await t.tap(find.text('Take photo'));
  await t.pumpAndSettle();
}

void main() {
  testWidgets(
    'granted: straight to the camera, nothing explained or requested',
    (t) async {
      final perms = FakePermissions();
      final picker = await _pump(t, perms);
      await _takePhoto(t);
      expect(picker.cameraCalls, 1);
      expect(perms.requested, isEmpty);
      expect(find.text('Continue'), findsNothing);
    },
  );

  testWidgets('not asked: explained first, the camera opens after Continue', (
    t,
  ) async {
    final perms = FakePermissions(
      states: {AtlasPermission.camera: PermissionState.notAsked},
    );
    final picker = await _pump(t, perms);
    await _takePhoto(t);

    expect(find.text('Allow the camera'), findsOneWidget);
    expect(find.textContaining('photograph a screen'), findsOneWidget);
    expect(perms.requested, isEmpty);
    expect(picker.cameraCalls, 0);

    await t.tap(find.text('Continue'));
    await t.pumpAndSettle();
    expect(perms.requested, [AtlasPermission.camera]);
    expect(picker.cameraCalls, 1);
  });

  testWidgets('Not now: no prompt, no camera', (t) async {
    final perms = FakePermissions(
      states: {AtlasPermission.camera: PermissionState.notAsked},
    );
    final picker = await _pump(t, perms);
    await _takePhoto(t);
    await t.tap(find.text('Not now'));
    await t.pumpAndSettle();
    expect(perms.requested, isEmpty);
    expect(picker.cameraCalls, 0);
  });

  testWidgets('refused in the prompt: the off state, not an error line', (
    t,
  ) async {
    final perms = FakePermissions(
      states: {AtlasPermission.camera: PermissionState.notAsked},
      answers: {AtlasPermission.camera: PermissionState.blocked},
    );
    final picker = await _pump(t, perms);
    await _takePhoto(t);
    await t.tap(find.text('Continue'));
    await t.pumpAndSettle();
    expect(picker.cameraCalls, 0);
    expect(find.text('Camera access is off'), findsOneWidget);
  });

  testWidgets('blocked: says so, Open Settings, and the library instead', (
    t,
  ) async {
    final perms = FakePermissions(
      states: {AtlasPermission.camera: PermissionState.blocked},
    );
    final picker = await _pump(t, perms);
    await _takePhoto(t);

    expect(find.text('Camera access is off'), findsOneWidget);
    expect(find.textContaining('take photos'), findsOneWidget);
    expect(picker.cameraCalls, 0);
    expect(perms.requested, isEmpty);

    await t.tap(find.text('Open Settings'));
    expect(perms.settingsOpened, 1);

    await t.tap(find.text('Choose from the library instead'));
    await t.pumpAndSettle();
    expect(picker.galleryCalls, 1);
    expect(find.text('Camera access is off'), findsNothing);
  });

  testWidgets('restricted: no Open Settings, the library is offered', (
    t,
  ) async {
    final picker = await _pump(
      t,
      FakePermissions(
        states: {AtlasPermission.camera: PermissionState.restricted},
      ),
    );
    await _takePhoto(t);
    expect(find.text('Restricted on this phone'), findsOneWidget);
    expect(find.text('Open Settings'), findsNothing);
    await t.tap(find.text('Choose from the library instead'));
    await t.pumpAndSettle();
    expect(picker.galleryCalls, 1);
  });

  testWidgets('the library never asks for anything', (t) async {
    final perms = FakePermissions(
      states: {AtlasPermission.camera: PermissionState.blocked},
    );
    final picker = await _pump(t, perms);
    await t.tap(find.text('Photo library'));
    await t.pumpAndSettle();
    expect(picker.galleryCalls, 1);
    expect(perms.requested, isEmpty);
    expect(find.text('Camera access is off'), findsNothing);
  });
}
