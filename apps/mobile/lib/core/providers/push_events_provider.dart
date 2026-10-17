import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';


/// Forwards all push events from the multiplexer for [device].
///
/// Yields nothing when the multiplexer is unavailable.
final devicePushEventsProvider = StreamProvider.autoDispose
    .family<Map<String, dynamic>, MacDevice>((ref, device) async* {
  final mux = await ref.watch(connectionNotifierProvider(device).future);
  if (mux == null) return;
  yield* mux.pushEvents;
});
