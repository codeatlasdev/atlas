import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

/// The page for a pushed route, with the platform's own transition: a [CupertinoPage] on iOS (slide in, edge
/// swipe back), a [MaterialPage] elsewhere (the transition the theme names). With Reduce motion it cuts.
///
/// Why this exists: go_router builds a `NoTransitionPage` unless it finds `material_ui`'s `MaterialApp`, and this
/// app runs the one in `flutter/material.dart`, so every push was an instant cut.
Page<void> routePage(BuildContext context, GoRouterState state, Widget child) {
  final key = state.pageKey;
  final name = state.name ?? state.path;
  final args = {...state.pathParameters, ...state.uri.queryParameters};
  if (MediaQuery.maybeDisableAnimationsOf(context) ?? false) {
    return NoTransitionPage<void>(
      key: key,
      name: name,
      arguments: args,
      restorationId: key.value,
      child: child,
    );
  }
  return switch (Theme.of(context).platform) {
    TargetPlatform.iOS || TargetPlatform.macOS => CupertinoPage<void>(
      key: key,
      name: name,
      arguments: args,
      restorationId: key.value,
      child: child,
    ),
    _ => MaterialPage<void>(
      key: key,
      name: name,
      arguments: args,
      restorationId: key.value,
      child: child,
    ),
  };
}
