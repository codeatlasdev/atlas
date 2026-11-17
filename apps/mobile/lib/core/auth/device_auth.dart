/// The "is it you?" check (Face ID, fingerprint, device passcode) before something that cannot be taken
/// back leaves the phone: typing into a Mac's terminal, handing a crewmate a secret. One seam so tests run
/// the real gate with an authenticator that says yes or no, never the plugin.
library;

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:local_auth/local_auth.dart';

import 'package:atlas_mobile/core/permissions/permission_copy.dart';
import 'package:atlas_mobile/core/permissions/permission_ui.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';

/// The phone cannot ask right now, and [message] says why and what to do. Nothing was authorised.
class DeviceAuthUnavailable implements Exception {
  const DeviceAuthUnavailable(this.message, {this.noLock = false});
  final String message;

  /// No passcode and no biometrics: the phone has nothing to ask with (the case Settings can fix).
  final bool noLock;

  @override
  String toString() => message;
}

/// Asks the person to prove it is them; `false` = they did not (cancelled). Throws [DeviceAuthUnavailable]
/// when the device cannot ask (no passcode set, locked out): the caller says so instead of sending anyway.
/// The passcode is always an accepted way: Face ID that is missing, not enrolled or refused falls back to it.
typedef DeviceAuth = Future<bool> Function(String reason);

/// What a failure of the lock plugin means for the person. Null: not a failure (they cancelled).
@visibleForTesting
DeviceAuthUnavailable? deviceAuthFailure(
  LocalAuthException e,
) => switch (e.code) {
  LocalAuthExceptionCode.userCanceled ||
  LocalAuthExceptionCode.systemCanceled ||
  LocalAuthExceptionCode.timeout ||
  LocalAuthExceptionCode.authInProgress => null,
  LocalAuthExceptionCode.noCredentialsSet => const DeviceAuthUnavailable(
    'This phone has no lock set up. Set a passcode in Settings, then try again.',
    noLock: true,
  ),
  LocalAuthExceptionCode.noBiometricsEnrolled ||
  LocalAuthExceptionCode.noBiometricHardware => const DeviceAuthUnavailable(
    'Face ID and fingerprint are not set up on this phone. Use your passcode, or set one up in Settings.',
  ),
  LocalAuthExceptionCode.biometricLockout => const DeviceAuthUnavailable(
    'Too many failed attempts. Enter your passcode to unlock, then try again.',
  ),
  LocalAuthExceptionCode.temporaryLockout => const DeviceAuthUnavailable(
    'Too many failed attempts. Try again in a moment.',
  ),
  _ => DeviceAuthUnavailable(
    'The lock could not be used (${e.description ?? e.code.name}).',
  ),
};

Future<bool> _platformAuth(String reason) async {
  try {
    return await LocalAuthentication().authenticate(localizedReason: reason);
  } on LocalAuthException catch (e) {
    final failure = deviceAuthFailure(e);
    if (failure == null) return false;
    throw failure;
  }
}

final deviceAuthProvider = Provider<DeviceAuth>((_) => _platformAuth);

/// Face ID / passcode in context: the first time on a phone where the system will ask for Face ID, Atlas says
/// why first. `true` = it is them; `false` = "Not now" or they cancelled (nothing to say). Throws
/// [DeviceAuthUnavailable] when this phone has no way to ask.
Future<bool> confirmItsYou(
  BuildContext context,
  WidgetRef ref,
  String reason,
) async {
  final state = await askPermission(context, ref, AtlasPermission.faceId);
  return switch (state) {
    PermissionState.granted => await ref.read(deviceAuthProvider)(reason),
    PermissionState.notAsked => false,
    PermissionState.blocked ||
    PermissionState.restricted ||
    PermissionState.unavailable => throw DeviceAuthUnavailable(
      '${permissionCopy(AtlasPermission.faceId).unavailableTitle}. '
      '${permissionCopy(AtlasPermission.faceId).unavailableHint}',
      noLock: true,
    ),
  };
}
