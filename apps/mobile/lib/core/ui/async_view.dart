/// The one way a screen renders an [AsyncValue]: loading, error with "Try again", empty, a computer
/// that is not connected, a protocol mismatch, else the data.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/computer_state.dart';
import 'package:atlas_mobile/core/ui/problem.dart';

/// Renders [value] in the app's one set of states:
///
/// - loading (no data yet): the one spinner ([AsyncLoading]);
/// - a [DeviceOffline] error: [ReachingView] with "Try again" (the retry);
/// - a [DeviceProtocolMismatch] error: the update message, no retry (it would fail the same way);
/// - any other error: [EmptyState] titled [errorTitle], the error's text, "Try again" when [onRetry] is set;
/// - data for which [isEmpty] says so: [EmptyState] with [emptyTitle] / [emptyMessage];
/// - else [data].
///
/// Data that is being reloaded keeps showing (the spinner is for "nothing yet"). Riverpod's `when` only does
/// that for a refresh (`invalidate`/`refresh`) by default; a reload (a provider rebuilt because something it
/// watches changed, which is what a reconnect does to everything behind `requireMux`) shows the loader unless
/// `skipLoadingOnReload` is set, so it is set here.
class AsyncView<T> extends ConsumerWidget {
  const AsyncView({
    super.key,
    required this.value,
    required this.data,
    required this.errorTitle,
    this.loading,
    this.onRetry,
    this.isEmpty,
    this.emptyTitle = 'Nothing here',
    this.emptyMessage = '',
    this.emptyIcon,
    this.emptyAction,
  });

  final AsyncValue<T> value;
  final Widget Function(T data) data;

  /// Title of the generic error state: "Couldn't read the audit". Its verb phrase ("read the audit") is what
  /// the error state says the phone was doing.
  final String errorTitle;

  /// What to show while nothing is loaded yet: the screen's own skeleton. Null is a quiet spinner.
  final Widget? loading;

  /// Called by "Try again"; usually `() => ref.invalidate(provider)`. Null hides the button.
  final VoidCallback? onRetry;

  /// Whether the loaded data is "nothing": shows the empty state instead of [data].
  final bool Function(T data)? isEmpty;
  final String emptyTitle;
  final String emptyMessage;
  final Widget? emptyIcon;
  final Widget? emptyAction;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return value.when(
      skipLoadingOnReload: true,
      loading: () =>
          loading ?? const Center(child: CircularProgressIndicator()),
      error: (e, _) => switch (e) {
        DeviceOffline(:final device) => ReachingView(
          device: device,
          connecting: false,
        ),
        _ => ProblemView(
          problem: Problem.of(
            e,
            doing: _doing(errorTitle),
            computer: switch (e) {
              DeviceUnavailable(:final device) => device.name,
              _ => null,
            },
          ),
          onRetry: e is DeviceProtocolMismatch ? null : onRetry,
        ),
      },
      data: (d) => isEmpty?.call(d) == true
          ? EmptyState(
              icon: emptyIcon,
              title: emptyTitle,
              message: emptyMessage,
              action: emptyAction,
            )
          : data(d),
    );
  }
}

/// "Couldn't read your inbox" → "read your inbox".
String _doing(String title) {
  final t = title.replaceFirst(
    RegExp(r"^Couldn'?t\s+", caseSensitive: false),
    '',
  );
  return t.isEmpty ? 'do that' : t[0].toLowerCase() + t.substring(1);
}
