import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';


/// Fetches the list of active PTYs from the connected Mac.
final ptyListProvider = FutureProvider.autoDispose
    .family<List<PtySummary>, MacDevice>((ref, device) async {
  final mux = await ref.watch(connectionNotifierProvider(device).future);
  if (mux == null) return [];
  final resp = await mux.call('pty_list');
  if (resp['ok'] != true) return [];
  final list = resp['data']['ptys'] as List<dynamic>;
  return list
      .map((e) => PtySummary.fromJson(e as Map<String, dynamic>))
      .toList();
});
