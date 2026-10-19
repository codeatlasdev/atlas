import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';

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

/// A project row: avatar, path and a one-line status.
class ProjectCard extends StatelessWidget {
  const ProjectCard({super.key, required this.device, required this.project});
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
        '/project',
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
