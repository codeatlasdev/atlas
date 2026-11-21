import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:material_ui/material_ui.dart';

/// Whether a popup (bottom sheet, dialog, menu) is open on any of the tabs' navigators.
///
/// The floating tab bar is drawn by the shell above the tabs' navigators, so a sheet a tab screen opens with
/// `showModalBottomSheet(context: …)` (whose default is the nearest navigator, the tab's own) would be drawn under
/// the bar: its buttons hidden and the bar still tappable over its scrim (the "Find your computer on Wi-Fi" sheet
/// in Settings, 2026-11-20). The shell hides the bar while [open] is true, so no screen, present or future, has
/// to remember anything: every popup route of every tab is seen here.
///
/// One [observer] per navigator (a [NavigatorObserver] can be attached to one navigator only); they share the set.
class PopupWatch {
  final _open = <Route<dynamic>>{};
  final ValueNotifier<bool> _any = ValueNotifier(false);

  /// True while at least one popup route is on a tab's navigator.
  ValueListenable<bool> get open => _any;

  /// A new observer for one navigator.
  NavigatorObserver observer() => _PopupObserver(this);

  void _add(Route<dynamic>? r) {
    if (r is PopupRoute) _open.add(r);
    _any.value = _open.isNotEmpty;
  }

  void _remove(Route<dynamic>? r) {
    if (r != null) _open.remove(r);
    _any.value = _open.isNotEmpty;
  }

  void dispose() => _any.dispose();
}

class _PopupObserver extends NavigatorObserver {
  _PopupObserver(this.watch);
  final PopupWatch watch;

  @override
  void didPush(Route<dynamic> route, Route<dynamic>? previousRoute) =>
      watch._add(route);

  @override
  void didPop(Route<dynamic> route, Route<dynamic>? previousRoute) =>
      watch._remove(route);

  @override
  void didRemove(Route<dynamic> route, Route<dynamic>? previousRoute) =>
      watch._remove(route);

  @override
  void didReplace({Route<dynamic>? newRoute, Route<dynamic>? oldRoute}) {
    watch._remove(oldRoute);
    watch._add(newRoute);
  }
}
