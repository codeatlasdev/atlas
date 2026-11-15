/// "Crewmate wants to run a tool": the bubble that carries Deny / Allow in the chat thread.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/attention_card.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
import 'package:atlas_mobile/features/crew/widgets/approval_parts.dart';

class CrewmateApprovalBubble extends StatelessWidget {
  const CrewmateApprovalBubble({
    super.key,
    required this.look,
    required this.approval,
    required this.onApprove,
  });

  final Map<String, dynamic>? look;
  final CrewmatePendingApproval approval;
  final void Function(bool) onApprove;

  @override
  Widget build(BuildContext context) {
    final cmd = approvalCommand(approval);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Crewmate(
            seed: approval.paneKey,
            look: look,
            state: CrewState.needsYou,
            size: 30,
          ),
          const SizedBox(width: 8),
          Flexible(
            child: AttentionCard(
              bubble: true,
              strength: AttentionStrength.firm,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  ApprovalToolLine(
                    approval: approval,
                    weight: FontWeight.w600,
                    flexible: true,
                  ),
                  if (cmd != null) ...[
                    const SizedBox(height: 6),
                    ApprovalCommandPreview(cmd),
                  ],
                  const SizedBox(height: 8),
                  ApprovalButtons(onAnswer: onApprove),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
