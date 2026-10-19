import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// A project's colour, stable for its name (avatar, header glow).
Color projectColor(BuildContext context, String name) {
  final series = AtlasColors.of(context).series;
  return series[name.codeUnits.fold(0, (a, b) => a + b) % series.length];
}

/// A project's letter avatar, coloured from its name like the desktop sidebar.
class ProjectAvatar extends StatelessWidget {
  const ProjectAvatar({super.key, required this.name, this.size = 40});
  final String name;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = projectColor(context, name);
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
