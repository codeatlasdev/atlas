import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/push/push_registration.dart';
import 'package:atlas_mobile/core/push/push_result.dart';
import 'package:atlas_mobile/core/push/push_relay.dart';
import 'package:atlas_mobile/core/push/push_service.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';

/// How the app gets a relay grant; tests override it.
final pushGrantFetcherProvider = Provider<PushGrantFetcher>(
  (ref) => fetchPushGrant,
);

/// Keeps the FCM token registered, with its grant, on every connected computer that can notify
/// this phone (issue #205), and answers the one line Settings shows about it: `null` when there is
/// nothing to say (no token: no Firebase config, or no computer connected yet), "Notifications are
/// on" once a computer took the registration, else the first reason one step failed. Watch it once
/// from the app root.
///
/// It runs again when the token changes (first read, FCM rotation) and whenever a computer's
/// connection is rebuilt (launch, reconnect), so a token an older app registered without a grant
/// is replaced on the next launch after the upgrade. The demo computer is skipped: it is answered
/// inside the app and has nothing to notify. A computer that fails does not stop the next one; the
/// failure is the line only when no computer registered.
final pushRegistrationProvider = FutureProvider<String?>((ref) async {
  final token = ref.watch(fcmTokenProvider);
  if (token == null) return null;
  final fetch = ref.read(pushGrantFetcherProvider);
  // The paired Macs and the crew servers behind them: crewmates, and so pushes, live on those.
  final devices = await ref.watch(crewDevicesProvider.future);
  var registered = false;
  String? why;
  for (final device in devices.where((d) => !isDemo(d))) {
    final mux = ref.watch(connectionNotifierProvider(device)).value;
    if (mux == null) continue;
    final can = await checkCanPush(mux);
    if (can is PushFail<void>) {
      why ??= can.reason;
      continue;
    }
    final done = await registerPush(mux, token, fetch);
    if (done is PushFail<void>) {
      why ??= done.reason;
    } else {
      registered = true;
    }
  }
  if (registered) return 'Notifications are on';
  return why == null ? null : 'Notifications are off: $why';
});
