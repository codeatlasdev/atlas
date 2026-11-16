import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/agent_approval.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/motion_size.dart';
import 'package:atlas_mobile/features/agents/approval_banner.dart';

/// What waits for an answer sits above the composer: the permission prompt and the last
/// send error. With the keyboard up on a short phone it scrolls inside the room that is
/// left ([maxHeight]), so the composer, which is where the user types, never leaves the
/// screen. Arriving later, it grows from the composer's edge instead of shoving the
/// thread in one frame, and just is there under Reduce motion.
class PendingZone extends StatelessWidget {
  const PendingZone({
    super.key,
    required this.approval,
    required this.error,
    required this.maxHeight,
    required this.onAnswer,
  });

  final AgentApproval? approval;
  final String? error;
  final double maxHeight;
  final void Function(AgentApproval approval, bool allow) onAnswer;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final approval = this.approval;
    return MotionSize(
      duration: Motion.slow,
      alignment: Alignment.bottomCenter,
      child: approval == null && error == null
          ? const SizedBox(width: double.infinity)
          : ConstrainedBox(
              constraints: BoxConstraints(maxHeight: maxHeight),
              child: SingleChildScrollView(
                child: Column(
                  children: [
                    if (approval != null)
                      ApprovalBanner(
                        approval: approval,
                        onAnswer: (allow) => onAnswer(approval, allow),
                      ),
                    if (error != null)
                      Padding(
                        padding: const EdgeInsets.symmetric(
                          horizontal: Space.md,
                        ),
                        child: Text(
                          error!,
                          style: Theme.of(context).textTheme.bodySmall!
                              .copyWith(color: c.error),
                        ),
                      ),
                  ],
                ),
              ),
            ),
    );
  }
}
