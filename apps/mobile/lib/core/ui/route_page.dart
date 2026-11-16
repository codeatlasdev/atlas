import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/backdrop.dart';

/// How a pushed screen arrives, decided once for every way the app opens one ([routePage] for go_router's pages,
/// [screenRoute] for the screens a screen pushes itself): the platform's own transition, and none at all with
/// Reduce motion.
enum _Arrival { cut, cupertino, material }

_Arrival _arrivalOf(BuildContext context) => Motion.reduced(context)
    ? _Arrival.cut
    : switch (Theme.of(context).platform) {
        TargetPlatform.iOS || TargetPlatform.macOS => _Arrival.cupertino,
        _ => _Arrival.material,
      };

/// The page for a pushed route, with the platform's own transition: a [CupertinoPage] on iOS (slide in, edge
/// swipe back), a [MaterialPage] elsewhere (the transition the theme names). With Reduce motion it cuts.
///
/// Why this exists: go_router builds a `NoTransitionPage` unless it finds `material_ui`'s `MaterialApp`, and this
/// app runs the one in `flutter/material.dart`, so every push was an instant cut.
Page<void> routePage(BuildContext context, GoRouterState state, Widget screen) {
  final key = state.pageKey;
  final name = state.name ?? state.path;
  final args = {...state.pathParameters, ...state.uri.queryParameters};
  // Opaque: a page sliding in covers the one under it, and every screen has the same light.
  final child = AtlasBackdrop(child: screen);
  return switch (_arrivalOf(context)) {
    _Arrival.cut => NoTransitionPage<void>(
      key: key,
      name: name,
      arguments: args,
      restorationId: key.value,
      child: child,
    ),
    _Arrival.cupertino => CupertinoPage<void>(
      key: key,
      name: name,
      arguments: args,
      restorationId: key.value,
      child: child,
    ),
    _Arrival.material => MaterialPage<void>(
      key: key,
      name: name,
      arguments: args,
      restorationId: key.value,
      child: child,
    ),
  };
}

/// The route for a screen one screen pushes onto another (`Navigator.push(screenRoute(context, (_) => X()))`),
/// with the same transition rules as [routePage]. Every push of a screen builds its route here, so none of them
/// can forget Reduce motion. [context] must be under the app's `MaterialApp` (a screen's own is).
PageRoute<T> screenRoute<T>(BuildContext context, WidgetBuilder builder) {
  Widget page(BuildContext context) => AtlasBackdrop(child: builder(context));
  return switch (_arrivalOf(context)) {
    _Arrival.cut => PageRouteBuilder<T>(
      pageBuilder: (context, _, _) => page(context),
      transitionDuration: Duration.zero,
      reverseTransitionDuration: Duration.zero,
    ),
    _Arrival.cupertino => CupertinoPageRoute<T>(builder: page),
    _Arrival.material => MaterialPageRoute<T>(builder: page),
  };
}

/// Pushes [screen] on the nearest navigator (the root one with [root]); completes when it pops.
Future<T?> pushScreen<T>(
  BuildContext context,
  Widget screen, {
  bool root = false,
}) => Navigator.of(
  context,
  rootNavigator: root,
).push(screenRoute<T>(context, (_) => screen));
