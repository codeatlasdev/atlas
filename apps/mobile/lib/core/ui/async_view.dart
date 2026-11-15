/// The one way a screen renders an [AsyncValue]: loading, error with "Try again", empty, a computer
/// that is not connected, a protocol mismatch, else the data.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/computer_state.dart';

/// Renders [value] in the app's one set of states:
///
/// - loading (no data yet): the one spinner ([AsyncLoading]);
/// - a [DeviceOffline] error: [ReachingView] with "Try again" (the retry);
/// - a [DeviceProtocolMismatch] error: the update message, no retry (it would fail the same way);
/// - any other error: [EmptyState] titled [errorTitle], the error's text, "Try again" when [onRetry] is set;
/// - data for which [isEmpty] says so: [EmptyState] with [emptyTitle] / [emptyMessage];
/// - else [data].
///
/// Data that is being refreshed keeps showing (the spinner is for "nothing yet"), as Riverpod's
/// `AsyncValue.when` does by default.
class AsyncView<T> extends ConsumerWidget {
  const AsyncView({
    super.key,
    required this.value,
    required this.data,
    required this.errorTitle,
    this.onRetry,
    this.isEmpty,
    this.emptyTitle = 'Nothing here',
    this.emptyMessage = '',
    this.emptyIcon,
    this.emptyAction,
  });

  final AsyncValue<T> value;
  final Widget Function(T data) data;

  /// Title of the generic error state: "Couldn't read the audit".
  final String errorTitle;

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
      loading: () => const Center(child: CircularProgressIndicator()),
      error: (e, _) => switch (e) {
        DeviceOffline(:final device) => ReachingView(
          device: device,
          connecting: false,
        ),
        DeviceProtocolMismatch(:final mismatch) => _MismatchView(mismatch),
        _ => EmptyState(
          title: errorTitle,
          message: errorText(e),
          action: onRetry == null
              ? null
              : OutlinedButton(
                  onPressed: onRetry,
                  child: const Text('Try again'),
                ),
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

class _MismatchView extends StatelessWidget {
  const _MismatchView(this.mismatch);
  final ProtocolMismatch mismatch;

  @override
  Widget build(BuildContext context) =>
      EmptyState(title: mismatch.title, message: mismatch.message);
}
