import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/projects/project_avatar.dart';

/// The panel while the first `project_detail` is on its way: the real header (the
/// list already knows the name, folder and services), then the screen's own blocks
/// shimmering in place, one tile per service, so nothing jumps when data lands.
class ProjectSkeleton extends StatefulWidget {
  const ProjectSkeleton({super.key, required this.project});
  final ProjectInfo project;

  @override
  State<ProjectSkeleton> createState() => _SkeletonState();
}

class _SkeletonState extends State<ProjectSkeleton>
    with SingleTickerProviderStateMixin {
  late final AnimationController _a = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1200),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (Motion.reduced(context)) {
      _a
        ..stop()
        ..value = 1;
    } else if (!_a.isAnimating) {
      _a.repeat(reverse: true);
    }
  }

  @override
  void dispose() {
    _a.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final p = widget.project;
    Widget bar(double h, {double? w, double r = Space.radius}) => Container(
      height: h,
      width: w,
      decoration: BoxDecoration(
        color: c.base,
        borderRadius: BorderRadius.circular(r),
      ),
    );
    Widget pulse(Widget w) => FadeTransition(
      opacity: Tween(begin: .45, end: 1.0).animate(_a),
      child: w,
    );
    final tiles = p.services.isEmpty ? 2 : p.services.length;
    return ListView(
      physics: const NeverScrollableScrollPhysics(),
      padding: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, Space.xxl),
      children: [
        Row(
          children: [
            Hero(
              tag: 'project-${p.id}',
              child: ProjectAvatar(name: p.name, size: 56),
            ),
            const SizedBox(width: Space.md),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    p.name,
                    overflow: TextOverflow.ellipsis,
                    style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                      fontWeight: FontWeight.w700,
                      letterSpacing: -.5,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    p.root.replaceFirst(RegExp(r'^/Users/[^/]+'), '~'),
                    overflow: TextOverflow.ellipsis,
                    style: Theme.of(context).textTheme.bodySmall!.copyWith(
                      color: c.muted,
                      fontFamily: monoFamily,
                      fontFamilyFallback: monoFallback,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
        const SizedBox(height: Space.md),
        pulse(
          Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Align(
                alignment: Alignment.centerLeft,
                child: bar(24, w: 110, r: 999),
              ),
              const SizedBox(height: Space.md + Space.sm),
              bar(76),
              const SizedBox(height: Space.md),
              bar(40, r: 999),
              const SizedBox(height: Space.lg),
              Align(alignment: Alignment.centerLeft, child: bar(12, w: 80)),
              const SizedBox(height: Space.sm + 2),
              LayoutBuilder(
                builder: (_, box) => Wrap(
                  spacing: Space.sm,
                  runSpacing: Space.sm,
                  children: [
                    for (var i = 0; i < tiles; i++)
                      SizedBox(
                        width: (box.maxWidth - Space.sm) / 2,
                        child: bar(104),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
