import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

/// The title line of an action card.
class ActionTitle extends StatelessWidget {
  const ActionTitle(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) => Text(
    text,
    style: Theme.of(context).textTheme.bodyMedium!.copyWith(
      fontWeight: FontWeight.w600,
      color: AtlasColors.of(context).text,
    ),
  );
}

/// The agent's reason under a title, when it gave one.
List<Widget> actionReason(BuildContext context, TaskAction action) => [
  if (action.reason != null) ...[
    const SizedBox(height: Space.xs),
    Text(
      action.reason!,
      style: Theme.of(context).textTheme.bodySmall!
          .copyWith(color: AtlasColors.of(context).muted),
    ),
  ],
];

/// Primary + Cancel, the row every kind but `choose` ends with. A null [onCancel] disables
/// Cancel (a send is in flight).
class ActionButtons extends StatelessWidget {
  const ActionButtons({
    super.key,
    required this.primary,
    required this.onPrimary,
    required this.onCancel,
    this.cancel = 'Cancel',
  });

  final String primary;
  final VoidCallback? onPrimary;
  final String cancel;
  final VoidCallback? onCancel;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      WarnPillButton(label: primary, onPressed: onPrimary),
      const SizedBox(width: Space.sm),
      OutlinePillButton(label: cancel, onPressed: onCancel),
    ],
  );
}
