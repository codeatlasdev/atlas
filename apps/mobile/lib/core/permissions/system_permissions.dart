/// The real [PermissionsPort]: the official plugin of each feature where it reports the state well
/// (`firebase_messaging` for notifications, `local_auth` for the device lock), `permission_handler` for the
/// camera, which no feature plugin can read without opening it, and for the microphone and speech recognition,
/// which Atlas's own dictation channel (`core/voice/`) never asks by itself. Photos need no permission at all
/// (`SystemImagePicker` uses the out-of-process system pickers).
library;

import 'package:firebase_core/firebase_core.dart';
import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter/foundation.dart';
import 'package:local_auth/local_auth.dart';
import 'package:permission_handler/permission_handler.dart' as ph;
import 'package:shared_preferences/shared_preferences.dart';

import 'package:atlas_mobile/core/permissions/permissions.dart';

/// `permission_handler`'s answer for the camera. iOS reports `denied` for "never asked" and
/// `permanentlyDenied` for a refusal; Android reports `denied` until the system stops asking.
@visibleForTesting
PermissionState fromHandler(ph.PermissionStatus s) => switch (s) {
  ph.PermissionStatus.granted ||
  ph.PermissionStatus.limited ||
  ph.PermissionStatus.provisional => PermissionState.granted,
  ph.PermissionStatus.denied => PermissionState.notAsked,
  ph.PermissionStatus.permanentlyDenied => PermissionState.blocked,
  ph.PermissionStatus.restricted => PermissionState.restricted,
};

/// Firebase's answer for notifications. iOS says `denied` once the person refused (it never asks twice);
/// Android says `denied` for a refusal it will still prompt again after, and `deniedPermanently` when it
/// will not.
@visibleForTesting
PermissionState fromAuthorization(AuthorizationStatus s, {required bool ios}) =>
    switch (s) {
      AuthorizationStatus.authorized ||
      AuthorizationStatus.provisional => PermissionState.granted,
      AuthorizationStatus.notDetermined => PermissionState.notAsked,
      AuthorizationStatus.denied =>
        ios ? PermissionState.blocked : PermissionState.notAsked,
      AuthorizationStatus.deniedPermanently => PermissionState.blocked,
    };

class SystemPermissions implements PermissionsPort {
  SystemPermissions({LocalAuthentication? auth, SharedPreferencesAsync? prefs})
    : _authOverride = auth,
      _prefsOverride = prefs;

  // Built on first use: the plugins exist only on a device, and a permission nobody asked about must not
  // need them.
  final LocalAuthentication? _authOverride;
  final SharedPreferencesAsync? _prefsOverride;
  late final LocalAuthentication _auth = _authOverride ?? LocalAuthentication();
  late final SharedPreferencesAsync _prefs =
      _prefsOverride ?? SharedPreferencesAsync();

  static bool get _ios => defaultTargetPlatform == TargetPlatform.iOS;

  static String _key(AtlasPermission p) => 'atlas_mobile.explained.${p.name}';

  @override
  Future<PermissionState> status(AtlasPermission p) async {
    try {
      return switch (p) {
        AtlasPermission.camera => fromHandler(
          await ph.Permission.camera.status,
        ),
        AtlasPermission.notifications => await _notifications(),
        AtlasPermission.microphone => fromHandler(
          await ph.Permission.microphone.status,
        ),
        // Android has no separate speech permission: RECORD_AUDIO covers its recognizers.
        AtlasPermission.speech =>
          _ios
              ? fromHandler(await ph.Permission.speech.status)
              : PermissionState.unavailable,
        AtlasPermission.faceId => await _faceId(),
        // iOS cannot be asked whether the Local Network switch is on; it is "allowed to dial" once the
        // person went through the explanation (the system prompt follows on the first dial).
        AtlasPermission.localNetwork =>
          !_ios
              ? PermissionState.unavailable
              : await explained(p)
              ? PermissionState.granted
              : PermissionState.notAsked,
      };
    } on Object catch (e) {
      debugPrint('atlas-mobile: permission ${p.name} status unreadable: $e');
      return PermissionState.unavailable;
    }
  }

  @override
  Future<PermissionState> request(AtlasPermission p) async {
    try {
      return switch (p) {
        AtlasPermission.camera => fromHandler(
          await ph.Permission.camera.request(),
        ),
        AtlasPermission.microphone => fromHandler(
          await ph.Permission.microphone.request(),
        ),
        AtlasPermission.speech =>
          _ios
              ? fromHandler(await ph.Permission.speech.request())
              : PermissionState.unavailable,
        AtlasPermission.notifications => fromAuthorization(
          (await FirebaseMessaging.instance.requestPermission(
            alert: true,
            badge: true,
            sound: true,
          )).authorizationStatus,
          ios: _ios,
        ),
        // The system asks for Face ID on the first authenticate, not here: this only records that the
        // person was told why.
        AtlasPermission.faceId ||
        AtlasPermission.localNetwork => await _explain(p),
      };
    } on Object catch (e) {
      debugPrint('atlas-mobile: permission ${p.name} request failed: $e');
      return PermissionState.unavailable;
    }
  }

  Future<PermissionState> _explain(AtlasPermission p) async {
    await markExplained(p);
    return status(p);
  }

  /// Not configured (no Firebase files in this build) is `unavailable`: there is nothing to ask for.
  Future<PermissionState> _notifications() async {
    if (Firebase.apps.isEmpty) return PermissionState.unavailable;
    final s = await FirebaseMessaging.instance.getNotificationSettings();
    return fromAuthorization(s.authorizationStatus, ios: _ios);
  }

  /// `notAsked` only where the system will show its own Face ID prompt on first use (iOS with a face
  /// sensor) and the person has not been told why yet. A phone with no passcode and no biometrics cannot
  /// unlock anything: `unavailable`. Every other lock (fingerprint, passcode) has no permission to give.
  Future<PermissionState> _faceId() async {
    if (!await _auth.isDeviceSupported()) return PermissionState.unavailable;
    if (_ios &&
        !await explained(AtlasPermission.faceId) &&
        (await _auth.getAvailableBiometrics()).contains(BiometricType.face)) {
      return PermissionState.notAsked;
    }
    return PermissionState.granted;
  }

  @override
  Future<void> openSettings() async {
    await ph.openAppSettings();
  }

  @override
  Future<bool> explained(AtlasPermission p) async =>
      await _prefs.getBool(_key(p)) ?? false;

  @override
  Future<void> markExplained(AtlasPermission p) =>
      _prefs.setBool(_key(p), true);
}
