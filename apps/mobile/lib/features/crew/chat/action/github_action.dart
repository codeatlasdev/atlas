import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_github.dart';
import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/action/action_parts.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

/// `connect_github`: the device-flow code to enter on GitHub while a login is running
/// ([flow] set), else the [buttons] that start it.
class ConnectGithubAction extends StatelessWidget {
  const ConnectGithubAction({
    super.key,
    required this.action,
    required this.flow,
    required this.polling,
    required this.onCancelFlow,
    required this.buttons,
  });

  final TaskAction action;
  final CrewGithubLogin? flow;
  final bool polling;
  final VoidCallback onCancelFlow;
  final Widget buttons;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final flow = this.flow;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const ActionTitle('Connect GitHub to continue'),
        ...actionReason(context, action),
        const SizedBox(height: Space.md),
        if (flow != null) ...[
          Text(
            'Enter this code at ${flow.verificationUri}',
            style: Theme.of(context).textTheme.bodySmall!
                .copyWith(color: c.muted),
          ),
          const SizedBox(height: Space.sm),
          Row(
            children: [
              Text(
                flow.userCode,
                style: Theme.of(context).textTheme.titleLarge!.copyWith(
                  fontWeight: FontWeight.w700,
                  letterSpacing: 2,
                  fontFamily: monoFamily,
                  fontFamilyFallback: monoFallback,
                  color: c.text,
                ),
              ),
              const SizedBox(width: Space.sm),
              OutlinedButton(
                onPressed: () {
                  // clipboard
                },
                style: OutlinedButton.styleFrom(
                  padding: const EdgeInsets.symmetric(
                    horizontal: Space.md,
                    vertical: Space.xs,
                  ),
                  minimumSize: Size.zero,
                  tapTargetSize: MaterialTapTargetSize.padded,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(8),
                  ),
                ),
                child: Text(
                  'Copy',
                  style: Theme.of(context).textTheme.bodySmall!
                      .copyWith(color: c.text),
                ),
              ),
            ],
          ),
          if (polling) ...[
            const SizedBox(height: Space.sm),
            Text(
              'Waiting for authorization…',
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: c.muted),
            ),
          ],
          const SizedBox(height: Space.sm),
          OutlinePillButton(
            label: 'Cancel',
            hPad: 12,
            vPad: 4,
            onPressed: onCancelFlow,
          ),
        ] else
          buttons,
      ],
    );
  }
}
