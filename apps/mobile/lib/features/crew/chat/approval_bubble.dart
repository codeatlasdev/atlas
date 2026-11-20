/// "Crewmate wants to run a tool": the bubble that carries the answers (Allow once, Allow for this task, Deny) in the
/// chat thread.
library;

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/attention_card.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
import 'package:atlas_mobile/features/crew/chat/voice_answer.dart';
import 'package:atlas_mobile/features/crew/permissions/approval_actions.dart';
import 'package:atlas_mobile/features/crew/widgets/approval_parts.dart';

class CrewmateApprovalBubble extends StatelessWidget {
  const CrewmateApprovalBubble({
    super.key,
    required this.look,
    required this.approval,
    required this.crewmateName,
    required this.onAnswer,
    this.voiceAnswer,
  });

  final Map<String, dynamic>? look;
  final CrewmatePendingApproval approval;
  final String crewmateName;
  final ApprovalAnswerer onAnswer;

  /// Voice answer to this approval (hostd decides what the words mean); null when the crew has voice off.
  final VoiceAnswerer? voiceAnswer;

  @override
  Widget build(BuildContext context) {
    final cmd = approvalCommand(approval);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Space.xs),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Crewmate(
            seed: approval.paneKey,
            look: look,
            state: CrewState.needsYou,
            size: 30,
          ),
          const SizedBox(width: Space.sm),
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
                    const SizedBox(height: Space.sm),
                    ApprovalCommandPreview(cmd),
                  ],
                  if (approval.rail != null) ...[
                    const SizedBox(height: Space.sm),
                    ApprovalRailNote(approval.rail!),
                  ],
                  const SizedBox(height: Space.sm),
                  ApprovalActions(
                    approval: approval,
                    crewmateName: crewmateName,
                    onAnswer: onAnswer,
                    voiceAnswer: voiceAnswer,
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
