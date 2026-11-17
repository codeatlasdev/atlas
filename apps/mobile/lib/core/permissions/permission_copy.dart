/// What the app says about each permission, in one place so every explanation, every "off" state and the
/// Settings rows use the same words. The iOS/Android usage strings (Info.plist, the manifest) say the same
/// thing in the system's own prompt.
library;

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/permissions/permissions.dart';

class PermissionCopy {
  const PermissionCopy({
    required this.icon,
    required this.title,
    required this.why,
    required this.private,
    required this.offTitle,
    required this.offWhat,
    required this.offHint,
    required this.restrictedHint,
    required this.unavailableTitle,
    required this.unavailableHint,
  });

  final IconData icon;

  /// The explanation's headline.
  final String title;

  /// One sentence: what it is for.
  final String why;

  /// What stays private.
  final String private;

  /// The state when the system will not ask again: its headline, what stops working, and the way forward.
  final String offTitle;
  final String offWhat;
  final String offHint;

  /// Shown instead of [offHint] when a device policy forbids it: Settings cannot change that.
  final String restrictedHint;

  /// This phone cannot do it at all (no camera, no lock set up): its headline and the way around it.
  final String unavailableTitle;
  final String unavailableHint;
}

PermissionCopy permissionCopy(AtlasPermission p) => switch (p) {
  AtlasPermission.camera => const PermissionCopy(
    icon: Icons.photo_camera_outlined,
    title: 'Allow the camera',
    why: 'Atlas reads the QR code on your computer to pair with it, and can photograph a screen for a crewmate.',
    private:
        'Nothing is recorded. A photo leaves this phone only when you send it.',
    offTitle: 'Camera access is off',
    offWhat: 'Atlas cannot scan the pairing code or take photos.',
    offHint: 'Turn on Camera for Atlas in Settings.',
    restrictedHint: 'A setting on this phone (like Screen Time) stops apps from using the camera.',
    unavailableTitle: 'No camera available',
    unavailableHint:
        'Paste the link printed under the QR code on your computer instead.',
  ),
  AtlasPermission.notifications => const PermissionCopy(
    icon: Icons.notifications_active_outlined,
    title: 'Get notified when you are away',
    why: 'Approvals and finished agents reach you even when Atlas is closed.',
    private: 'A notification says only what happened and where, like "Claude · my-project". What your agents write stays on your computer.',
    offTitle: 'Notifications are off',
    offWhat: 'Approvals and finished agents will not reach you while Atlas is closed.',
    offHint: 'Turn on Notifications for Atlas in Settings.',
    restrictedHint:
        'A setting on this phone stops apps from sending notifications.',
    unavailableTitle: 'Notifications are not available',
    unavailableHint: 'This build of Atlas has no push set up.',
  ),
  AtlasPermission.faceId => const PermissionCopy(
    icon: Icons.face_unlock_outlined,
    title: 'Confirm it is you',
    why: 'Before typing into your computer\'s terminal or sending a secret, Atlas checks it is really you.',
    private: 'Face ID stays on this phone: Atlas never sees your face, only whether it matched. Your passcode works too.',
    offTitle: 'This phone has no lock set up',
    offWhat: 'Atlas cannot confirm it is you, so typing into a terminal and sending secrets stay off.',
    offHint: 'Set a passcode in your phone\'s Settings, then try again.',
    restrictedHint: 'A setting on this phone stops apps from using it.',
    unavailableTitle: 'This phone has no lock set up',
    unavailableHint: "Set a passcode in your phone's Settings, then try again.",
  ),
  AtlasPermission.localNetwork => const PermissionCopy(
    icon: Icons.wifi_rounded,
    title: 'Find your computer on Wi-Fi',
    why: 'On the same network Atlas connects straight to your computer: faster, and without leaving the room.',
    private: 'Only your own computer is contacted. If you choose Don\'t Allow, Atlas still connects through an encrypted relay.',
    offTitle: 'Direct connection is off',
    offWhat: 'Atlas reaches your computer through the relay instead: it works, a little slower.',
    offHint: 'Turn on Local Network for Atlas in Settings.',
    restrictedHint:
        'A setting on this phone stops apps from using the local network.',
    unavailableTitle: 'Direct connection is not available',
    unavailableHint: 'Atlas reaches your computer through the relay.',
  ),
};
