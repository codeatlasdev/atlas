import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';

/// Shows the list of projects on a connected Mac.
class ProjectsScreen extends ConsumerWidget {
  const ProjectsScreen({super.key, required this.device});

  final MacDevice device;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final projectsAsync = ref.watch(projectListProvider(device));

    return RefreshIndicator(
      onRefresh: () async => ref.invalidate(projectListProvider(device)),
      child: projectsAsync.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('Error: $e')),
        data: (projects) {
          if (projects.isEmpty) {
            return const Center(child: Text('No projects found.'));
          }
          return ListView.builder(
            itemCount: projects.length,
            itemBuilder: (context, i) {
              final p = projects[i];
              return ListTile(
                leading: CircleAvatar(
                  backgroundColor: p.running ? Colors.green : Colors.grey,
                  radius: 6,
                  child: const SizedBox.shrink(),
                ),
                title: Text(p.name),
                subtitle: Text(
                  '${p.services.length} service${p.services.length == 1 ? '' : 's'}'
                  '  •  ${p.root}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
                onTap: () => context.push(
                  '/device/project',
                  extra: {'device': device, 'project': p},
                ),
              );
            },
          );
        },
      ),
    );
  }
}
