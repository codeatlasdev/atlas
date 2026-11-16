import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/route_page.dart';
import 'package:atlas_mobile/features/git/changes_screen.dart';

/// Entry point on the project page: opens the project's [ChangesScreen].
class ChangesTile extends StatelessWidget {
  const ChangesTile({super.key, required this.device, required this.project});

  final MacDevice device;
  final ProjectInfo project;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.only(top: Space.md),
      child: AtlasCard(
        key: const ValueKey('changes-tile'),
        onTap: () => pushScreen<void>(
          context,
          ChangesScreen(device: device, project: project),
        ),
        padding: const EdgeInsets.all(Space.md),
        child: Row(
          children: [
            Icon(Icons.difference_outlined, color: c.accent),
            const SizedBox(width: Space.md),
            const Expanded(
              child: Text(
                'Review changes',
                style: TextStyle(fontWeight: FontWeight.w600),
              ),
            ),
            Icon(Icons.chevron_right, color: c.faint),
          ],
        ),
      ),
    );
  }
}
