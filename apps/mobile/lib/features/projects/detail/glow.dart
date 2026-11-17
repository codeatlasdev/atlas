import 'package:material_ui/material_ui.dart';

/// The project's colour as a faint glow behind its header. Place it first in a [Stack].
class ProjectGlow extends StatelessWidget {
  const ProjectGlow({super.key, required this.tint});

  final Color tint;

  @override
  Widget build(BuildContext context) => Positioned(
    top: -180,
    left: -100,
    right: -100,
    height: 440,
    child: IgnorePointer(
      child: DecoratedBox(
        decoration: BoxDecoration(
          gradient: RadialGradient(
            colors: [tint.withValues(alpha: .2), tint.withValues(alpha: 0)],
          ),
        ),
      ),
    ),
  );
}
