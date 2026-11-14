import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/push/push_registration.dart';
import 'package:atlas_mobile/core/push/push_relay.dart';
import 'package:atlas_mobile/core/push/push_service.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';

/// How the app gets a relay grant; tests override it.
final pushGrantFetcherProvider = Provider<PushGrantFetcher>(
  (ref) => fetchPushGrant,
);

/// Keeps the FCM token registered, with its grant, on every connected computer that can notify
/// this phone (issue #205). Watch it once from the app root.
///
/// It runs again when the token changes (first read, FCM rotation) and whenever a computer's
/// connection is rebuilt (launch, reconnect), so a token an older app registered without a grant
/// is replaced on the next launch after the upgrade. The demo computer is skipped: it is answered
/// inside the app and has nothing to notify.
final pushRegistrationProvider = FutureProvider<void>((ref) async {
  final token = ref.watch(fcmTokenProvider);
  if (token == null) return;
  final fetch = ref.read(pushGrantFetcherProvider);
  // The paired Macs and the crew servers behind them: crewmates, and so pushes, live on those.
  final devices = await ref.watch(crewDevicesProvider.future);
  for (final device in devices.where((d) => !isDemo(d))) {
    final mux = ref.watch(connectionNotifierProvider(device)).value;
    if (mux == null || !await checkCrewCap(mux)) continue;
    await registerPush(mux, token, fetch);
  }
});
