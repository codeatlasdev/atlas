/// The Crew tab's "Captain's call" card: a crewmate waiting on a tool approval, or a task parked on a card it
/// cannot get past alone (a guard refused it, a UI task without captures), answerable from the list. The second
/// shows the captures the agent took (tap to enlarge) and why it waits; Retry and Override answer in place, and a
/// tap on the card opens the chat, where a marked screenshot can be added to a retry.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/attention_card.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
import 'package:atlas_mobile/features/crew/chat/task_images.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/widgets/approval_parts.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

/// Whether [e] has something for the Captain's call: an approval, or a card that can be retried.
bool needsCaptain(CrewEntry e) =>
    e.crewmate.pendingApproval != null ||
    (e.crewmate.waitingTask?.retryable ?? false);

class NeedsYouCard extends StatelessWidget {
  const NeedsYouCard({
    super.key,
    required this.entry,
    required this.onAnswer,
    required this.onAnswerTask,
    required this.onTap,
  });

  final CrewEntry entry;
  final void Function(bool allow) onAnswer;

  /// Retry or Override (`retry` / `override`) for the waiting task's card.
  final void Function(String value) onAnswerTask;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final pa = entry.crewmate.pendingApproval;
    final cmd = approvalCommand(pa);
    final task = entry.crewmate.waitingTask;

    // One button for the whole card, named for what it asks; Deny / Allow / Retry stay their own buttons
    // inside it. The InkWell's own tap node is dropped so the card is not announced twice.
    final label = pa != null
        ? '${entry.crewmate.name} wants to run ${pa.toolName ?? 'a tool'}'
        : '${entry.crewmate.name} needs you';
    return Semantics(
      container: true,
      explicitChildNodes: true,
      button: true,
      label: label,
      onTap: onTap,
      child: InkWell(
        onTap: onTap,
        excludeFromSemantics: true,
        child: AttentionCard(
          margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 3),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Crewmate(
                    seed: entry.crewmate.id,
                    look: entry.crewmate.look,
                    state: CrewState.needsYou,
                    size: 32,
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      entry.crewmate.name,
                      style: Theme.of(context).textTheme.bodyLarge!
                          .copyWith(fontWeight: FontWeight.w700, color: c.text),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              if (pa != null) ...[
                ApprovalToolLine(
                  approval: pa,
                  weight: FontWeight.w500,
                  flexible: true,
                ),
                if (cmd != null) ...[
                  const SizedBox(height: 6),
                  ApprovalCommandPreview(cmd),
                ],
                const SizedBox(height: 10),
                ApprovalButtons(onAnswer: onAnswer, hPad: 16, vPad: 6, gap: 8),
              ] else if (task != null && task.retryable)
                ..._waiting(context, c, task),
            ],
          ),
        ),
      ),
    );
  }

  /// The card of a parked task: what it is, the agent's captures, the reasons, Retry / Override.
  List<Widget> _waiting(
    BuildContext context,
    AtlasColors c,
    CrewmateWaitingTask task,
  ) {
    final unproven = task.action.kind == 'prove_ui';
    final reasons = unproven
        ? [if (task.action.reason != null) task.action.reason!]
        : task.action.reasons;
    return [
      Text(
        unproven ? 'Could not prove this UI work' : 'This work was refused',
        style: Theme.of(context).textTheme.bodyMedium!
            .copyWith(fontWeight: FontWeight.w600, color: c.text),
      ),
      TaskImageRow(
        images: task.attachments,
        taskId: task.id,
        device: entry.device,
        crewmateId: entry.crewmate.id,
        alignEnd: false,
      ),
      const SizedBox(height: 6),
      for (final r in reasons)
        Padding(
          padding: const EdgeInsets.only(bottom: 2),
          child: Text(
            '• $r',
            style: Theme.of(context).textTheme.bodySmall!
                .copyWith(color: c.muted),
          ),
        ),
      const SizedBox(height: 10),
      Row(
        children: [
          WarnPillButton(
            label: 'Retry',
            onPressed: () => onAnswerTask('retry'),
          ),
          const SizedBox(width: 8),
          OutlinePillButton(
            label: 'Override',
            onPressed: () => onAnswerTask('override'),
          ),
        ],
      ),
    ];
  }
}
