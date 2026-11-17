import 'package:atlas_mobile/core/permissions/permissions.dart';

/// The platform side of permissions, scripted: what each permission's state is, what a system prompt
/// answers, and a log of every call, so a test can say "nothing was asked" or "asked once, after the
/// explanation".
class FakePermissions implements PermissionsPort {
  FakePermissions({
    Map<AtlasPermission, PermissionState>? states,
    Map<AtlasPermission, PermissionState>? answers,
  }) : states = {...?states},
       answers = {...?answers};

  /// The state each permission is in now (granted when not set).
  final Map<AtlasPermission, PermissionState> states;

  /// What the system prompt answers per permission (granted when not set); it becomes the new state.
  final Map<AtlasPermission, PermissionState> answers;

  final requested = <AtlasPermission>[];
  final explainedSet = <AtlasPermission>{};
  var settingsOpened = 0;

  @override
  Future<PermissionState> status(AtlasPermission p) async =>
      states[p] ?? PermissionState.granted;

  @override
  Future<PermissionState> request(AtlasPermission p) async {
    requested.add(p);
    return states[p] = answers[p] ?? PermissionState.granted;
  }

  @override
  Future<void> openSettings() async => settingsOpened++;

  @override
  Future<bool> explained(AtlasPermission p) async => explainedSet.contains(p);

  @override
  Future<void> markExplained(AtlasPermission p) async => explainedSet.add(p);
}
