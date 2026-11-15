import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';

/// One thing in this project that waits on the user, with its action.
class ProjectBanner extends StatelessWidget {
  const ProjectBanner({
    super.key,
    required this.color,
    required this.icon,
    required this.title,
    required this.action,
    required this.onTap,
  });
  final Color color;
  final IconData icon;
  final String title;
  final String action;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: Space.sm),
    child: Semantics(
      button: true,
      label: '$title, $action',
      excludeSemantics: true,
      onTap: onTap,
      child: GestureDetector(
        onTap: onTap,
        child: Container(
          constraints: const BoxConstraints(minHeight: 48),
          alignment: Alignment.centerLeft,
          padding: const EdgeInsets.fromLTRB(
            Space.md,
            Space.sm,
            Space.sm,
            Space.sm,
          ),
          decoration: BoxDecoration(
            color: color.withValues(alpha: .12),
            borderRadius: BorderRadius.circular(Space.radius),
            border: Border.all(color: color.withValues(alpha: .45)),
          ),
          child: Row(
            children: [
              Icon(icon, color: color, size: 18),
              const SizedBox(width: Space.sm),
              Expanded(
                child: Text(
                  title,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
              ),
              AtlasChip(action, tone: color),
            ],
          ),
        ),
      ),
    ),
  );
}
