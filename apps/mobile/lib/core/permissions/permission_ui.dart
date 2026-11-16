/// The one way Atlas asks for a permission: tell the person why, right before the feature needs it, and only
/// then show the system prompt (Apple 5.1.1, Google's permission guidance). And the one way it shows a
/// permission that is off: what stops working and the way forward, never a blank screen.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/permissions/permission_copy.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// Asks for [p] in the flow of the feature that needs it.
///
/// - Already allowed: returns [PermissionState.granted] at once, nothing shown.
/// - Not asked: the explanation sheet first; "Continue" shows the system prompt and returns what the person
///   answered; "Not now" (Android only, see [explanationCanDecline]) returns [PermissionState.notAsked] and
///   nothing was requested.
/// - Blocked, restricted, unavailable: returns that state without a prompt (the system would not show one);
///   the caller shows [PermissionOffCard] or [showPermissionOff].
///
/// [why] replaces the sentence of the explanation when the feature has a more specific one.
Future<PermissionState> askPermission(
  BuildContext context,
  WidgetRef ref,
  AtlasPermission p, {
  String? why,
}) async {
  final port = ref.read(permissionsProvider);
  final now = await port.status(p);
  if (now != PermissionState.notAsked || !context.mounted) return now;
  final go = await showPermissionExplainer(context, p, why: why);
  if (go != true) return PermissionState.notAsked;
  final after = await port.request(p);
  if (ref.context.mounted) ref.invalidate(permissionStateProvider(p));
  return after;
}

/// After the first pairing the app says what notifications are for and asks, once: not while pairing (the
/// person is busy with the QR) and not at launch. A "Not now" is remembered, so a second pairing does not
/// ask again; Settings → This phone always can. Nothing happens when they are already on, refused (only
/// Settings can change that) or not available in this build.
Future<void> offerNotifications(BuildContext context, WidgetRef ref) async {
  final port = ref.read(permissionsProvider);
  const p = AtlasPermission.notifications;
  if (await port.status(p) != PermissionState.notAsked ||
      await port.explained(p) ||
      !context.mounted) {
    return;
  }
  await port.markExplained(p);
  if (context.mounted) await askPermission(context, ref, p);
}

/// Whether the explanation before a system prompt may offer a way out ("Not now", tap outside, swipe down).
///
/// - iOS: no. The HIG ("Requesting permission") wants one button, Continue, and no way to leave the screen
///   without seeing the system alert; App Review rejects a "Continue" + "Not Now" pre-prompt under 5.1.1(iv)
///   (Apple Developer Forums thread 817672, March 2026). The person declines in the system alert, and the
///   feature's screen keeps its own alternatives (Paste link, the relay).
/// - Android: yes. Google: "Always provide the option to cancel an educational UI flow."
bool explanationCanDecline(BuildContext context) =>
    Theme.of(context).platform != TargetPlatform.iOS;

/// The explanation as a bottom sheet. `true` = "Continue", anything else = "Not now" or dismissed (Android).
Future<bool?> showPermissionExplainer(
  BuildContext context,
  AtlasPermission p, {
  String? why,
}) {
  final decline = explanationCanDecline(context);
  return showModalBottomSheet<bool>(
    context: context,
    isScrollControlled: true,
    isDismissible: decline,
    enableDrag: decline,
    showDragHandle: decline,
    backgroundColor: AtlasColors.of(context).overlay,
    builder: (ctx) => SafeArea(
      child: SingleChildScrollView(
        padding: EdgeInsets.fromLTRB(
          Space.xl,
          decline ? 0 : Space.xl,
          Space.xl,
          Space.lg,
        ),
        child: PermissionExplainer(
          permission: p,
          why: why,
          onContinue: () => Navigator.of(ctx).pop(true),
          onNotNow: () => Navigator.of(ctx).pop(false),
        ),
      ),
    ),
  );
}

/// Icon, one sentence on why, what stays private, "Continue" (shows the system prompt) and, where the
/// platform allows it ([explanationCanDecline]), "Not now". Used in the sheet and inline (the pairing screen
/// is the camera's explanation until it is allowed).
class PermissionExplainer extends StatelessWidget {
  const PermissionExplainer({
    super.key,
    required this.permission,
    required this.onContinue,
    required this.onNotNow,
    this.why,
  });

  final AtlasPermission permission;
  final String? why;
  final VoidCallback onContinue;
  final VoidCallback onNotNow;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
    final copy = permissionCopy(permission);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _IconBadge(copy.icon, c.accent),
        const SizedBox(height: Space.lg),
        Text(copy.title, style: t.titleMedium, textAlign: TextAlign.center),
        const SizedBox(height: Space.sm),
        Text(
          why ?? copy.why,
          style: t.bodyLarge!.copyWith(color: c.muted, height: 1.4),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Space.lg),
        Container(
          padding: const EdgeInsets.all(Space.md),
          decoration: BoxDecoration(
            color: c.base,
            borderRadius: BorderRadius.circular(Space.radius),
            border: Border.all(color: c.borderSubtle),
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.lock_outline_rounded, size: 16, color: c.success),
              const SizedBox(width: Space.sm),
              Expanded(
                child: Text(
                  copy.private,
                  style: t.bodyMedium!.copyWith(color: c.muted, height: 1.4),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: Space.xl),
        FilledButton(onPressed: onContinue, child: const Text('Continue')),
        if (explanationCanDecline(context)) ...[
          const SizedBox(height: Space.xs),
          TextButton(onPressed: onNotNow, child: const Text('Not now')),
        ],
      ],
    );
  }
}

/// What a permission that is off means for the person: what stops working and the way forward. [extra]
/// are the fallbacks the feature has (paste the link, choose from the library), under "Open Settings".
class PermissionOffCard extends ConsumerWidget {
  const PermissionOffCard({
    super.key,
    required this.permission,
    required this.state,
    this.extra = const [],
  });

  final AtlasPermission permission;

  /// [PermissionState.blocked], [PermissionState.restricted] or [PermissionState.unavailable].
  final PermissionState state;
  final List<Widget> extra;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
    final copy = permissionCopy(permission);
    final restricted = state == PermissionState.restricted;
    final unavailable = state == PermissionState.unavailable;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _IconBadge(copy.icon, c.warning),
        const SizedBox(height: Space.lg),
        Text(
          restricted
              ? 'Restricted on this phone'
              : unavailable
              ? copy.unavailableTitle
              : copy.offTitle,
          style: t.titleMedium,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Space.sm),
        Text(
          copy.offWhat,
          style: t.bodyLarge!.copyWith(color: c.muted, height: 1.4),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Space.sm),
        Text(
          restricted
              ? copy.restrictedHint
              : unavailable
              ? copy.unavailableHint
              : copy.offHint,
          style: t.bodyMedium!.copyWith(color: c.muted, height: 1.4),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Space.xl),
        // Settings can change a refusal; it cannot change a policy or a missing lock.
        if (!restricted && !unavailable) ...[
          FilledButton(
            onPressed: ref.read(permissionsProvider).openSettings,
            child: const Text('Open Settings'),
          ),
          const SizedBox(height: Space.xs),
        ],
        ...extra,
      ],
    );
  }
}

/// [PermissionOffCard] as a sheet, for a feature that is not a whole screen (a photo from the composer). The
/// [extra] fallbacks close it with a value (`Navigator.pop(sheet, value)`), which this returns; "Close" and a
/// swipe return null.
Future<T?> showPermissionOff<T>(
  BuildContext context,
  AtlasPermission p,
  PermissionState state, {
  List<Widget> Function(BuildContext sheet) extra = _noExtra,
}) => showModalBottomSheet<T>(
  context: context,
  isScrollControlled: true,
  showDragHandle: true,
  backgroundColor: AtlasColors.of(context).overlay,
  builder: (ctx) => SafeArea(
    child: SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(Space.xl, 0, Space.xl, Space.lg),
      child: PermissionOffCard(
        permission: p,
        state: state,
        extra: [
          ...extra(ctx),
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(),
            child: const Text('Close'),
          ),
        ],
      ),
    ),
  ),
);

List<Widget> _noExtra(BuildContext _) => const [];

class _IconBadge extends StatelessWidget {
  const _IconBadge(this.icon, this.tone);
  final IconData icon;
  final Color tone;

  @override
  Widget build(BuildContext context) => Center(
    child: Container(
      width: 64,
      height: 64,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: tone.withValues(alpha: .14),
      ),
      child: Icon(icon, size: 30, color: tone),
    ),
  );
}

/// Runs [onResume] when the app comes back to the foreground: the person may have flipped a switch in the
/// system Settings, and the screen that showed "Off" must show the truth when they return.
class PermissionResumeRefresh extends StatefulWidget {
  const PermissionResumeRefresh({
    super.key,
    required this.onResume,
    required this.child,
  });

  final VoidCallback onResume;
  final Widget child;

  @override
  State<PermissionResumeRefresh> createState() =>
      _PermissionResumeRefreshState();
}

class _PermissionResumeRefreshState extends State<PermissionResumeRefresh> {
  late final AppLifecycleListener _life = AppLifecycleListener(
    onResume: () => widget.onResume(),
  );

  @override
  void initState() {
    super.initState();
    _life;
  }

  @override
  void dispose() {
    _life.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
