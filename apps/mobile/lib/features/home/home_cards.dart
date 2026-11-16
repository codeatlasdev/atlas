import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';

/// An installed agent whose hooks are off: its tabs never show here until it is turned on.
class HomeIntegrationOff extends StatelessWidget {
  const HomeIntegrationOff({super.key, required this.agent});
  final AgentIntegration agent;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return AtlasCard(
      accent: c.warning,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.link_off_rounded, color: c.warning, size: 20),
          const SizedBox(width: Space.sm),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text("${agent.label} integration is off"),
                const SizedBox(height: Space.xs),
                Text(
                  "Its agents won't show here. Accept Atlas's offer on your Mac, or run `atlas setup ${agent.setup} --apply` there.",
                  style: TextStyle(color: c.muted),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// The list is empty because the computer answered "none": how to add a project.
class HomeNoProjects extends StatelessWidget {
  const HomeNoProjects({super.key});

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return EmptyState(
      icon: Icon(Icons.folder_outlined, size: 56, color: c.faint),
      title: 'No projects yet',
      message: 'Open a folder in Atlas on your Mac and it shows up here.',
      action: Container(
        padding: const EdgeInsets.symmetric(
          horizontal: Space.lg,
          vertical: Space.sm,
        ),
        decoration: BoxDecoration(
          color: c.raised,
          borderRadius: BorderRadius.circular(Space.radius),
        ),
        child: Text(
          'Atlas Desktop: File → Open Folder\nor run `atlas .` in a project',
          style: Theme.of(context).textTheme.bodySmall!.copyWith(
            color: c.muted,
            fontFamily: monoFamily,
            fontFamilyFallback: monoFallback,
          ),
          textAlign: TextAlign.center,
        ),
      ),
    );
  }
}
