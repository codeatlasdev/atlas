/// The one way to get a live connection to a computer.
///
/// Every command goes through a [Multiplexer]; the connection may be opening, down, or refused for a
/// protocol it cannot speak. [requireMux] (providers) and [requireMuxOf] (widgets) wait for an opening
/// link and hand back the live [Multiplexer], or throw ONE typed error a screen can recognise and word
/// once: [DeviceOffline], or [DeviceProtocolMismatch] when the computer answered but speaks another
/// protocol (#302). Nobody checks `mux == null` and words their own "is offline" any more.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';

/// Why a command cannot run: there is no live connection to [device].
sealed class DeviceUnavailable implements Exception {
  const DeviceUnavailable(this.device);
  final MacDevice device;

  /// The one sentence a screen shows (a snackbar, an inline error, a [StateError]'s text).
  String get message;

  @override
  String toString() => message;
}

/// The computer is off, asleep, out of reach, or still being reached.
class DeviceOffline extends DeviceUnavailable {
  const DeviceOffline(super.device);

  @override
  String get message => '${device.name} is offline';
}

/// The computer answered, but its hostd and this app disagree on the protocol: it needs an update,
/// not a retry.
class DeviceProtocolMismatch extends DeviceUnavailable {
  const DeviceProtocolMismatch(super.device, this.mismatch);
  final ProtocolMismatch mismatch;

  @override
  String get message => mismatch.title;
}

/// The text to show for [error]: [DeviceUnavailable] and [StateError] carry their own message, anything
/// else is its `toString`. The one place a screen turns a caught error into words.
String errorText(Object error) => switch (error) {
  DeviceUnavailable(:final message) => message,
  StateError(:final message) => message,
  _ => '$error',
};

/// `mux`, or the typed error that says why there is none.
Multiplexer _live(
  Multiplexer? mux,
  MacDevice device,
  ProtocolMismatch? mismatch,
) {
  if (mux != null) return mux;
  throw mismatch == null
      ? DeviceOffline(device)
      : DeviceProtocolMismatch(device, mismatch);
}

/// The live connection to [device], for providers. Waits while it opens. Watches, so a provider that
/// calls it rebuilds when the connection does.
Future<Multiplexer> requireMux(Ref ref, MacDevice device) async {
  final mux = await ref.watch(connectionNotifierProvider(device).future);
  return _live(mux, device, ref.read(protocolMismatchProvider(device)));
}

/// The live connection to [device], for widgets and their handlers. Waits while it opens; reads, so it
/// is safe in an event handler.
Future<Multiplexer> requireMuxOf(WidgetRef ref, MacDevice device) async {
  final mux = await ref.read(connectionNotifierProvider(device).future);
  return _live(mux, device, ref.read(protocolMismatchProvider(device)));
}

/// The live connection to [device] right now, or null: for handlers that quietly do nothing while the
/// computer is off (background refreshes, a button pressed on a stale screen) and must not wait for a
/// link that is still opening. Anything that tells the user why it did not run uses [requireMuxOf].
Multiplexer? liveMux(WidgetRef ref, MacDevice device) =>
    ref.read(connectionNotifierProvider(device)).value;
