import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// Atlas Lab: the second app `bun run mobile:lab` installs beside the real one (docs/mobile.md,
/// "Lab lane"). It is the same code, built with `--dart-define=ATLAS_LAB=1 --dart-define=ATLAS_SHA=<git sha>`;
/// the native side gives it its own bundle id (`…mobile.lab`), name and icon, so its pairings and secure
/// storage never touch the real app's. The real build passes neither define and is unchanged.
@immutable
class LabConfig {
  const LabConfig({required this.enabled, required this.sha});

  /// What this build was told: `ATLAS_LAB` / `ATLAS_SHA` (compile-time, so a release has neither).
  factory LabConfig.fromEnvironment() => const LabConfig(
    enabled: bool.fromEnvironment('ATLAS_LAB'),
    sha: String.fromEnvironment('ATLAS_SHA'),
  );

  final bool enabled;
  final String sha;

  /// Characters of the commit shown (git's own short form is 7+; 12 keeps a long ref readable).
  static const _shaChars = 12;

  /// `LAB · ` and the short sha, or just `LAB` for a build made outside `bun run mobile:lab`.
  String get banner {
    final s = sha.trim();
    if (s.isEmpty) return 'LAB';
    return 'LAB · ${s.length > _shaChars ? s.substring(0, _shaChars) : s}';
  }
}

/// Wraps the whole app (`MaterialApp.builder`): in a lab build, a slim strip on top says
/// "LAB" and the commit so the maintainer never confuses it with the real app; otherwise it is the app, untouched.
class LabFrame extends StatelessWidget {
  const LabFrame({super.key, required this.lab, required this.child});

  final LabConfig lab;
  final Widget child;

  static const height = 22.0;

  @override
  Widget build(BuildContext context) {
    if (!lab.enabled) return child;
    final c = AtlasColors.of(context);
    final top = MediaQuery.paddingOf(context).top;
    return Column(
      children: [
        // Orange like the Lab icon, under the status bar (the app below has no inset left to take).
        Container(
          key: const ValueKey('lab-banner'),
          color: c.warning,
          width: double.infinity,
          height: height + top,
          padding: EdgeInsets.only(top: top),
          alignment: Alignment.center,
          child: Text(
            lab.banner,
            style: TextStyle(
              color: c.onAccent,
              fontSize: 11,
              fontWeight: FontWeight.w700,
              letterSpacing: 1.2,
              fontFamily: monoFamily,
              fontFamilyFallback: monoFallback,
            ),
          ),
        ),
        Expanded(
          // The status bar is the strip's now: the app must not pad for it a second time.
          child: MediaQuery.removePadding(
            context: context,
            removeTop: true,
            child: child,
          ),
        ),
      ],
    );
  }
}
