import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';

/// Atlas's palette, taken from the desktop's bundled theme
/// (`apps/desktop/src/lib/themes/atlas.json`, Atlas Dark / Atlas Light, Tokyo Night).
/// Every screen reads colours through [AtlasColors.of], never literals.
@immutable
class AtlasColors extends ThemeExtension<AtlasColors> {
  const AtlasColors({
    required this.ground,
    required this.base,
    required this.raised,
    required this.overlay,
    required this.text,
    required this.muted,
    required this.faint,
    required this.accent,
    required this.onAccent,
    required this.border,
    required this.borderSubtle,
    required this.hover,
    required this.success,
    required this.warning,
    required this.info,
    required this.error,
    required this.series,
  });

  /// `background`: the chrome behind everything.
  final Color ground;

  /// `editor.background`: cards and content panes.
  final Color base;

  /// `surface.background`.
  final Color raised;

  /// `elevated_surface.background`: sheets, menus, dialogs.
  final Color overlay;
  final Color text;
  final Color muted;
  final Color faint;
  final Color accent;
  final Color onAccent;
  final Color border;
  final Color borderSubtle;
  final Color hover;
  final Color success;
  final Color warning;
  final Color info;
  final Color error;

  /// Chart series (terminal ANSI blue, magenta, green, yellow, cyan, red).
  final List<Color> series;

  static const dark = AtlasColors(
    ground: Color(0xFF0B1320),
    base: Color(0xFF131C2B),
    raised: Color(0xFF1D2637),
    overlay: Color(0xFF273143),
    text: Color(0xFFC0CAF5),
    muted: Color(0xFF9AA7CC),
    faint: Color(0xFF6F7C9F),
    accent: Color(0xFF87ADFB),
    onAccent: Color(0xFF0B1320),
    border: Color(0x29C0CAF5),
    borderSubtle: Color(0x14C0CAF5),
    hover: Color(0x13C0CAF5),
    success: Color(0xFFA0CF6C),
    warning: Color(0xFFEF945F),
    info: Color(0xFF77C8F8),
    error: Color(0xFFFB8399),
    series: [
      Color(0xFF7AA2F7),
      Color(0xFFBB9AF7),
      Color(0xFF9ECE6A),
      Color(0xFFE0AF68),
      Color(0xFF7DCFFF),
      Color(0xFFF7768E),
    ],
  );

  static AtlasColors of(BuildContext context) =>
      Theme.of(context).extension<AtlasColors>()!;

  /// Service state → colour, the desktop's `--dot-*` mapping.
  Color serviceState(String state) => switch (state) {
    'running' => success,
    'starting' => warning,
    'failed' => error,
    _ => faint,
  };

  /// Agent state → colour (working = accent, needs you = warning, done = success).
  Color agentState(String state) => switch (state) {
    'working' => accent,
    'blocked' || 'waiting' => warning,
    'done' => success,
    _ => faint,
  };

  @override
  AtlasColors copyWith() => this;

  @override
  AtlasColors lerp(AtlasColors? other, double t) =>
      t < .5 ? this : other ?? this;
}

/// The desktop's motion vocabulary (`app.css`): one spring-ish ease-out, three speeds.
abstract final class Motion {
  static const ease = Cubic(0.2, 0.8, 0.2, 1);
  static const fast = Duration(milliseconds: 120);
  static const normal = Duration(milliseconds: 200);
  static const slow = Duration(milliseconds: 260);

  /// Lists build in with this gap between items.
  static const stagger = Duration(milliseconds: 40);

  /// `prefers-reduced-motion`: animations collapse to their end state.
  static bool reduced(BuildContext context) =>
      MediaQuery.maybeDisableAnimationsOf(context) ?? false;
}

/// Spacing scale (4-pt) and radii shared by every screen.
abstract final class Space {
  static const xs = 4.0;
  static const sm = 8.0;
  static const md = 12.0;
  static const lg = 16.0;
  static const xl = 24.0;
  static const xxl = 32.0;
  static const radius = 12.0;
  static const radiusSm = 8.0;
}

/// Monospace for ports, commands, logs: the platform's (SF Mono on iOS).
const monoFamily = 'Menlo';
const monoFallback = ['SF Mono', 'RobotoMono', 'monospace'];

/// Atlas is dark only, like the desktop app.
ThemeData atlasTheme() {
  const c = AtlasColors.dark;
  const brightness = Brightness.dark;
  final scheme = ColorScheme(
    brightness: brightness,
    primary: c.accent,
    onPrimary: c.onAccent,
    secondary: c.accent,
    onSecondary: c.onAccent,
    error: c.error,
    onError: c.onAccent,
    surface: c.base,
    onSurface: c.text,
    onSurfaceVariant: c.muted,
    surfaceContainerLowest: c.ground,
    surfaceContainerLow: c.base,
    surfaceContainer: c.raised,
    surfaceContainerHigh: c.raised,
    surfaceContainerHighest: c.overlay,
    outline: c.border,
    outlineVariant: c.borderSubtle,
    primaryContainer: c.accent.withValues(alpha: .18),
    onPrimaryContainer: c.text,
  );
  final base = ThemeData(
    useMaterial3: true,
    brightness: brightness,
    colorScheme: scheme,
    extensions: [c],
  );
  final text = base.textTheme.apply(bodyColor: c.text, displayColor: c.text);
  return base.copyWith(
    scaffoldBackgroundColor: c.ground,
    canvasColor: c.ground,
    splashFactory: InkSparkle.splashFactory,
    textTheme: text.copyWith(
      titleLarge: text.titleLarge?.copyWith(
        fontWeight: FontWeight.w600,
        letterSpacing: -.3,
      ),
      titleMedium: text.titleMedium?.copyWith(fontWeight: FontWeight.w600),
      titleSmall: text.titleSmall?.copyWith(
        fontWeight: FontWeight.w600,
        color: c.muted,
        letterSpacing: .2,
      ),
      bodySmall: text.bodySmall?.copyWith(color: c.muted),
      labelSmall: text.labelSmall?.copyWith(color: c.faint, letterSpacing: .3),
    ),
    appBarTheme: AppBarTheme(
      backgroundColor: c.ground,
      surfaceTintColor: Colors.transparent,
      foregroundColor: c.text,
      elevation: 0,
      scrolledUnderElevation: 0,
      centerTitle: false,
      titleTextStyle: text.titleLarge?.copyWith(
        fontWeight: FontWeight.w600,
        fontSize: 20,
      ),
    ),
    dividerTheme: DividerThemeData(
      color: c.borderSubtle,
      space: 1,
      thickness: 1,
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: c.accent,
        foregroundColor: c.onAccent,
        minimumSize: const Size(0, 44),
        padding: const EdgeInsets.symmetric(horizontal: Space.lg),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(Space.radiusSm),
        ),
        textStyle: const TextStyle(fontWeight: FontWeight.w600),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: c.text,
        side: BorderSide(color: c.border),
        minimumSize: const Size(0, 40),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(Space.radiusSm),
        ),
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(foregroundColor: c.accent),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: c.base,
      isDense: true,
      contentPadding: const EdgeInsets.symmetric(
        horizontal: Space.md,
        vertical: Space.md,
      ),
      hintStyle: TextStyle(color: c.faint),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(Space.radius),
        borderSide: BorderSide(color: c.border),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(Space.radius),
        borderSide: BorderSide(color: c.border),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(Space.radius),
        borderSide: BorderSide(color: c.accent, width: 1.5),
      ),
    ),
    snackBarTheme: SnackBarThemeData(
      behavior: SnackBarBehavior.floating,
      backgroundColor: c.text,
      contentTextStyle: TextStyle(color: c.base),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Space.radius),
      ),
    ),
    bottomSheetTheme: BottomSheetThemeData(
      backgroundColor: c.overlay,
      surfaceTintColor: Colors.transparent,
      showDragHandle: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
      ),
    ),
    dialogTheme: DialogThemeData(
      backgroundColor: c.overlay,
      surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(16)),
    ),
    popupMenuTheme: PopupMenuThemeData(
      color: c.overlay,
      surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Space.radius),
        side: BorderSide(color: c.border),
      ),
    ),
    segmentedButtonTheme: SegmentedButtonThemeData(
      style: SegmentedButton.styleFrom(
        backgroundColor: c.base,
        selectedBackgroundColor: c.raised,
        selectedForegroundColor: c.text,
        foregroundColor: c.muted,
        side: BorderSide(color: c.border),
        visualDensity: VisualDensity.compact,
      ),
    ),
    progressIndicatorTheme: ProgressIndicatorThemeData(color: c.accent),
    pageTransitionsTheme: const PageTransitionsTheme(
      builders: {
        TargetPlatform.iOS: CupertinoPageTransitionsBuilder(),
        TargetPlatform.android: FadeForwardsPageTransitionsBuilder(),
      },
    ),
  );
}
