import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/action/action_parts.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

/// `choose`: one lettered button per option, then Cancel task.
class ChooseAction extends StatelessWidget {
  const ChooseAction({
    super.key,
    required this.action,
    required this.busy,
    required this.onPick,
    required this.onCancel,
  });

  final TaskAction action;
  final bool busy;
  final void Function(String option) onPick;
  final VoidCallback onCancel;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        ActionTitle(action.question ?? 'Choose an option'),
        const SizedBox(height: Space.sm),
        for (int i = 0; i < action.options.length; i++) ...[
          OutlinedButton(
            onPressed: busy ? null : () => onPick(action.options[i]),
            style: OutlinedButton.styleFrom(
              foregroundColor: c.text,
              side: BorderSide(color: c.borderSubtle),
              padding: const EdgeInsets.symmetric(
                horizontal: Space.md,
                vertical: Space.sm,
              ),
              minimumSize: const Size(double.infinity, 48),
              tapTargetSize: MaterialTapTargetSize.padded,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(10),
              ),
              alignment: Alignment.centerLeft,
            ),
            child: Row(
              children: [
                Container(
                  width: 22,
                  height: 22,
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                    color: c.ground,
                    borderRadius: BorderRadius.circular(4),
                    border: Border.all(color: c.borderSubtle),
                  ),
                  child: Text(
                    String.fromCharCode(65 + i),
                    style: Theme.of(context).textTheme.labelSmall!
                        .copyWith(fontWeight: FontWeight.w600, color: c.muted),
                  ),
                ),
                const SizedBox(width: Space.md),
                Flexible(
                  child: Text(
                    action.options[i],
                    style: Theme.of(context).textTheme.bodyLarge!
                        .copyWith(color: c.text),
                  ),
                ),
              ],
            ),
          ),
          if (i < action.options.length - 1) const SizedBox(height: Space.xs),
        ],
        const SizedBox(height: Space.sm),
        OutlinePillButton(
          label: 'Cancel task',
          onPressed: busy ? null : onCancel,
        ),
      ],
    );
  }
}
