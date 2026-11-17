/// What the phone lets Atlas do (camera, notifications, the device lock, the local network), in one place:
/// each permission's state, asking for it, and opening the app's page in the system Settings.
///
/// Nothing here shows UI and nothing is asked on its own: screens ask through `askPermission`
/// (`permission_ui.dart`), after an explanation, right before the feature needs it. [PermissionsPort] is the
/// one seam to the platform, so tests drive the real screens with a fake and never load a plugin.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/permissions/system_permissions.dart';

/// Every permission the app can trigger. `docs/mobile.md` ("Permissions") says when each one fires.
enum AtlasPermission {
  /// Scanning the pairing QR and photographing a screen for a crewmate.
  camera,

  /// Approvals and finished agents reaching the phone while the app is closed.
  notifications,

  /// Face ID, fingerprint or the device passcode before typing into a terminal or sending a secret.
  faceId,

  /// iOS only: reaching the computer directly on the same Wi-Fi.
  localNetwork,
}

/// Where a permission stands, from the point of view of what the screen should show.
enum PermissionState {
  /// Allowed: use the feature.
  granted,

  /// Nothing refused yet (or Android denied once and may be asked again): explain, then ask.
  notAsked,

  /// Refused and the system will not ask again (always on iOS): only the system Settings can change it.
  blocked,

  /// A device policy (Screen Time, MDM) forbids it; the Settings switch cannot change that.
  restricted,

  /// This phone cannot do it (no camera, no lock set up, push not configured in this build, not iOS).
  unavailable,
}

/// The platform side of permissions. [SystemPermissions] is the real one.
abstract interface class PermissionsPort {
  /// The current state; never shows a prompt.
  Future<PermissionState> status(AtlasPermission p);

  /// Shows the system prompt (when the system still allows one) and returns the state after it.
  Future<PermissionState> request(AtlasPermission p);

  /// Opens this app's page in the system Settings.
  Future<void> openSettings();

  /// Whether the person has been through Atlas's explanation of [p]. iOS has no way to read the Face ID or
  /// Local Network state, and the notification offer after pairing happens once: this is that memory. It is
  /// per install (a reinstall asks again, like the system does).
  Future<bool> explained(AtlasPermission p);

  Future<void> markExplained(AtlasPermission p);
}

final permissionsProvider = Provider<PermissionsPort>(
  (ref) => SystemPermissions(),
);

/// The state of one permission, for the screens that show it. Invalidate it after asking, and when the app
/// returns from the system Settings (`PermissionResumeRefresh`).
final permissionStateProvider = FutureProvider.autoDispose
    .family<PermissionState, AtlasPermission>(
      (ref, p) => ref.watch(permissionsProvider).status(p),
    );
