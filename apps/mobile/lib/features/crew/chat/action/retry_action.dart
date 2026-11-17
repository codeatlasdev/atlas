import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/action/action_parts.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';
import 'package:atlas_mobile/features/crew/chat/task_images.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

/// A card that can be retried: Atlas's guards refused the work, or a UI task finished without two captures.
/// The captures the agent took (tap to enlarge), the reasons, a marked screenshot the owner may add, then
/// Retry (the task runs again with the reasons in its prompt and the screenshot staged for the agent) or
/// Override (accept the work as it is; images never go with it).
class RetryAction extends StatelessWidget {
  const RetryAction({
    super.key,
    required this.title,
    required this.reasons,
    required this.task,
    required this.device,
    required this.crewmateId,
    required this.images,
    required this.busy,
    required this.onRemoveImage,
    required this.onAddImage,
    required this.onRetry,
    required this.onOverride,
  });

  final String title;
  final List<String> reasons;
  final CrewmateTask task;
  final MacDevice device;
  final String crewmateId;

  /// The screenshots the owner added, sent with Retry.
  final List<PendingImage> images;
  final bool busy;
  final void Function(int index) onRemoveImage;
  final VoidCallback onAddImage;
  final VoidCallback onRetry;
  final VoidCallback onOverride;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        ActionTitle(title),
        TaskImageRow(
          images: task.agentImages,
          taskId: task.id,
          device: device,
          crewmateId: crewmateId,
          alignEnd: false,
        ),
        const SizedBox(height: Space.sm),
        for (final reason in reasons)
          Padding(
            padding: const EdgeInsets.only(bottom: 2),
            child: Text(
              '• $reason',
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: c.muted),
            ),
          ),
        const SizedBox(height: Space.sm),
        if (images.isNotEmpty)
          Wrap(
            spacing: 6,
            runSpacing: 4,
            children: [
              for (var i = 0; i < images.length; i++)
                InputChip(
                  label: Text(
                    images[i].name,
                    style: Theme.of(context).textTheme.bodySmall!
                        .copyWith(color: c.text),
                  ),
                  onDeleted: busy ? null : () => onRemoveImage(i),
                  deleteButtonTooltipMessage: 'Remove ${images[i].name}',
                ),
            ],
          ),
        OutlinePillButton(
          label: 'Add a screenshot',
          onPressed: busy ? null : onAddImage,
        ),
        const SizedBox(height: Space.sm),
        Row(
          children: [
            WarnPillButton(label: 'Retry', onPressed: busy ? null : onRetry),
            const SizedBox(width: Space.sm),
            OutlinePillButton(
              label: 'Override',
              onPressed: busy ? null : onOverride,
            ),
          ],
        ),
      ],
    );
  }
}
