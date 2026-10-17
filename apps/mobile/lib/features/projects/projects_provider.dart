import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';


/// Fetches the list of projects from the connected Mac.
final projectListProvider = FutureProvider.autoDispose
    .family<List<ProjectInfo>, MacDevice>((ref, device) async {
  final mux = await ref.watch(connectionNotifierProvider(device).future);
  if (mux == null) return [];
  final resp = await mux.call('project_list');
  if (resp['ok'] != true) return [];
  final list = resp['data']['projects'] as List<dynamic>;
  return list
      .map((e) => ProjectInfo.fromJson(e as Map<String, dynamic>))
      .toList();
});
