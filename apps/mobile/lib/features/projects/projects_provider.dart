import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/projects/project_metrics.dart';

/// Fetches the list of projects from the connected Mac.
///
/// Throws when it cannot: [DeviceUnavailable] while the computer is not connected, a [StateError]
/// with hostd's message when it refuses. An empty list means the computer answered "no projects"
/// and nothing else, so a screen can tell the two apart.
final projectListProvider = FutureProvider.autoDispose
    .family<List<ProjectInfo>, MacDevice>((ref, device) async {
      final mux = await requireMux(ref, device);
      final resp = await mux.call('project_list');
      if (resp['ok'] != true) {
        throw StateError(
          resp['error'] as String? ?? 'Could not read the projects',
        );
      }
      final list =
          (resp['data'] as Map<dynamic, dynamic>?)?['projects']
              as List<dynamic>? ??
          const [];
      return list
          .map((e) => ProjectInfo.fromJson(e as Map<String, dynamic>))
          .toList();
    });

/// The last `project_detail` answer of each project, by (computer, project id): a project opened again draws
/// what it last knew on its first frame and updates in place when the fresh answer lands, instead of a
/// skeleton for a round trip.
final projectSnapshotCacheProvider =
    Provider<Map<(String, String), ProjectSnapshot>>((_) => {});
