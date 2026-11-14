/// The Crew tab's "Captain's call" card: a crewmate waiting on a tool approval, answerable
/// from the list.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/widgets/approval_parts.dart';

class NeedsYouCard extends StatelessWidget {
  const NeedsYouCard({
    super.key,
    required this.entry,
    required this.onAnswer,
    required this.onTap,
  });

  final CrewEntry entry;
  final void Function(bool allow) onAnswer;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final pa = entry.crewmate.pendingApproval;
    final cmd = approvalCommand(pa);

    return InkWell(
      onTap: onTap,
      child: Container(
        margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 3),
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          color: c.warning.withValues(alpha: .08),
          border: Border.all(color: c.warning.withValues(alpha: .35)),
          borderRadius: BorderRadius.circular(14),
        ),
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
                Text(
                  entry.crewmate.name,
                  style: TextStyle(
                    fontWeight: FontWeight.w700,
                    fontSize: 14,
                    color: c.text,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            ApprovalToolLine(approval: pa, weight: FontWeight.w500),
            if (cmd != null) ...[
              const SizedBox(height: 6),
              ApprovalCommandPreview(cmd),
            ],
            const SizedBox(height: 10),
            ApprovalButtons(onAnswer: onAnswer, hPad: 16, vPad: 6, gap: 8),
          ],
        ),
      ),
    );
  }
}
