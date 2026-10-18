import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';

/// A project's letter avatar, coloured from its name like the desktop sidebar.
class ProjectAvatar extends StatelessWidget {
  const ProjectAvatar({super.key, required this.name, this.size = 40});
  final String name;
  final double size;

  @override
  Widget build(BuildContext context) {
    final series = AtlasColors.of(context).series;
    final k = series[name.codeUnits.fold(0, (a, b) => a + b) % series.length];
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: k.withValues(alpha: .18),
        borderRadius: BorderRadius.circular(size * .28),
      ),
      child: Text(
        name.isEmpty ? '?' : name.characters.first.toUpperCase(),
        style: TextStyle(
          color: k,
          fontWeight: FontWeight.w700,
          fontSize: size * .45,
        ),
      ),
    );
  }
}

/// A Mac's projects, refreshed every few seconds while visible.
class ProjectsScreen extends ConsumerStatefulWidget {
  const ProjectsScreen({super.key, required this.device});

  final MacDevice device;

  @override
  ConsumerState<ProjectsScreen> createState() => _ProjectsScreenState();
}

class _ProjectsScreenState extends ConsumerState<ProjectsScreen> {
  late final Timer _timer = Timer.periodic(
    const Duration(seconds: 5),
    (_) => ref.invalidate(projectListProvider(widget.device)),
  );

  @override
  void initState() {
    super.initState();
    _timer;
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final projects = ref.watch(projectListProvider(widget.device));
    final list = projects.value;
    return RefreshIndicator(
      color: c.accent,
      onRefresh: () async => ref.invalidate(projectListProvider(widget.device)),
      child: switch (list) {
        null when projects.hasError => ListView(
          children: [
            EmptyState(
              title: 'Could not load projects',
              message: '${projects.error}',
            ),
          ],
        ),
        null => const Center(child: CircularProgressIndicator()),
        [] => ListView(
          children: const [
            EmptyState(
              title: 'No projects yet',
              message: 'Projects you add in Atlas Desktop show up here.',
            ),
          ],
        ),
        _ => ListView.builder(
          padding: const EdgeInsets.fromLTRB(
            Space.lg,
            Space.sm,
            Space.lg,
            Space.xxl,
          ),
          itemCount: list.length,
          itemBuilder: (context, i) => EnterFade(
            index: i,
            child: _ProjectCard(device: widget.device, project: list[i]),
          ),
        ),
      },
    );
  }
}

class _ProjectCard extends StatelessWidget {
  const _ProjectCard({required this.device, required this.project});
  final MacDevice device;
  final ProjectInfo project;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final up = project.services.where((s) => s.state == 'running').length;
    final failed = project.services.where((s) => s.state == 'failed').length;
    final status = !project.running
        ? ('Stopped', c.faint)
        : failed > 0
        ? ('$failed failed', c.error)
        : ('$up/${project.services.length} running', c.success);
    return AtlasCard(
      padding: const EdgeInsets.all(Space.md),
      accent: failed > 0 && project.running ? c.error : null,
      onTap: () => context.push(
        '/device/project',
        extra: {'device': device, 'project': project},
      ),
      child: Row(
        children: [
          ProjectAvatar(name: project.name),
          const SizedBox(width: Space.md),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  project.name,
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const SizedBox(height: 2),
                Text(
                  project.root.replaceFirst(RegExp(r'^/Users/[^/]+'), '~'),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: c.faint,
                    fontSize: 12,
                    fontFamily: monoFamily,
                    fontFamilyFallback: monoFallback,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(width: Space.sm),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              StatusDot(
                color: status.$2,
                size: 7,
                pulse: project.running && failed == 0,
              ),
              Text(status.$1, style: TextStyle(color: c.muted, fontSize: 12)),
            ],
          ),
        ],
      ),
    );
  }
}
