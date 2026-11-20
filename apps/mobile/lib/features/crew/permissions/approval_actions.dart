/// How a person answers a crewmate's tool approval: Allow once (the primary), Allow for this task, Deny, each one
/// tap, and a way to add a comment the agent receives (a bottom sheet). There is no "always": no answer grants
/// standing access, and a call a rail held can only be allowed once.
library;

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/permission_mode.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/features/crew/widgets/approval_parts.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

/// One answer: allow or deny, how far an allow reaches, and the comment that goes with it.
class ApprovalDecision {
  const ApprovalDecision({
    required this.allow,
    this.scope = ApprovalScope.once,
    this.comment,
  });

  final bool allow;
  final ApprovalScope scope;
  final String? comment;
}

/// Answers an approval; resolves when hostd has taken the answer (or the screen has said why not).
typedef ApprovalAnswerer = Future<void> Function(ApprovalDecision decision);

/// The three answers as pills, and "Comment or more options" under them.
class ApprovalActions extends StatelessWidget {
  const ApprovalActions({
    super.key,
    required this.approval,
    required this.crewmateName,
    required this.onAnswer,
    this.hPad = 14,
    this.vPad = 5,
    this.gap = 6,
  });

  final CrewmatePendingApproval approval;
  final String crewmateName;
  final ApprovalAnswerer onAnswer;
  final double hPad;
  final double vPad;
  final double gap;

  Future<void> _more(BuildContext context) async {
    final decision = await showApprovalAnswerSheet(
      context,
      approval: approval,
      crewmateName: crewmateName,
    );
    if (decision != null) await onAnswer(decision);
  }

  @override
  Widget build(BuildContext context) {
    final l = context.l10n;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        Wrap(
          alignment: WrapAlignment.end,
          crossAxisAlignment: WrapCrossAlignment.center,
          spacing: gap,
          runSpacing: 4,
          children: [
            OutlinePillButton(
              label: l.mobile_crew_approval_deny,
              danger: true,
              hPad: hPad,
              vPad: vPad,
              onPressed: () => onAnswer(const ApprovalDecision(allow: false)),
            ),
            if (approval.rail == null)
              OutlinePillButton(
                label: l.mobile_crew_approval_allow_task,
                hPad: hPad,
                vPad: vPad,
                onPressed: () => onAnswer(
                  const ApprovalDecision(
                    allow: true,
                    scope: ApprovalScope.task,
                  ),
                ),
              ),
            WarnPillButton(
              label: l.mobile_crew_approval_allow_once,
              hPad: hPad,
              vPad: vPad,
              onPressed: () => onAnswer(const ApprovalDecision(allow: true)),
            ),
          ],
        ),
        TextButton(
          onPressed: () => _more(context),
          child: Text(l.mobile_crew_approval_more),
        ),
      ],
    );
  }
}

/// The answer sheet: what is asked, the three answers at full width and an optional comment the agent receives
/// with the answer. Resolves with the decision, or null when it is dismissed.
Future<ApprovalDecision?> showApprovalAnswerSheet(
  BuildContext context, {
  required CrewmatePendingApproval approval,
  required String crewmateName,
}) => showModalBottomSheet<ApprovalDecision>(
  context: context,
  isScrollControlled: true,
  useSafeArea: true,
  showDragHandle: true,
  builder: (_) => _AnswerSheet(approval: approval, crewmateName: crewmateName),
);

class _AnswerSheet extends StatefulWidget {
  const _AnswerSheet({required this.approval, required this.crewmateName});
  final CrewmatePendingApproval approval;
  final String crewmateName;

  @override
  State<_AnswerSheet> createState() => _AnswerSheetState();
}

class _AnswerSheetState extends State<_AnswerSheet> {
  final _comment = TextEditingController();

  @override
  void dispose() {
    _comment.dispose();
    super.dispose();
  }

  void _answer(bool allow, [ApprovalScope scope = ApprovalScope.once]) =>
      Navigator.of(context).pop(
        ApprovalDecision(allow: allow, scope: scope, comment: _comment.text),
      );

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final approval = widget.approval;
    final cmd = approvalCommand(approval);
    return SingleChildScrollView(
      padding: EdgeInsets.fromLTRB(
        Space.xl,
        Space.sm,
        Space.xl,
        Space.xl + MediaQuery.viewInsetsOf(context).bottom,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Semantics(
            header: true,
            child: Text(
              l.mobile_crew_approval_sheet_title(widget.crewmateName),
              style: Theme.of(context).textTheme.titleLarge,
            ),
          ),
          const SizedBox(height: Space.md),
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
          const SizedBox(height: Space.lg),
          TextField(
            key: const Key('approval-comment'),
            controller: _comment,
            minLines: 2,
            maxLines: 5,
            maxLength: approvalCommentMax,
            maxLengthEnforcement: MaxLengthEnforcement.enforced,
            buildCounter: (
              _, {
              required currentLength,
              required isFocused,
              maxLength,
            }) => null,
            textCapitalization: TextCapitalization.sentences,
            decoration: InputDecoration(
              labelText: l.mobile_crew_approval_comment_label,
              hintText: l.mobile_crew_approval_comment_hint,
            ),
          ),
          const SizedBox(height: Space.lg),
          FilledButton(
            key: const Key('approval-sheet-allow-once'),
            onPressed: () => _answer(true),
            child: Text(l.mobile_crew_approval_allow_once),
          ),
          if (approval.rail == null) ...[
            const SizedBox(height: Space.sm),
            OutlinedButton(
              key: const Key('approval-sheet-allow-task'),
              onPressed: () => _answer(true, ApprovalScope.task),
              child: Text(l.mobile_crew_approval_allow_task),
            ),
            Padding(
              padding: const EdgeInsets.only(top: Space.xs),
              child: Text(
                l.mobile_crew_approval_task_note,
                style: Theme.of(context).textTheme.bodySmall!
                    .copyWith(color: c.muted, height: 1.35),
              ),
            ),
          ],
          const SizedBox(height: Space.sm),
          OutlinedButton(
            key: const Key('approval-sheet-deny'),
            style: OutlinedButton.styleFrom(
              foregroundColor: c.error,
              side: BorderSide(color: c.error.withValues(alpha: .5)),
            ),
            onPressed: () => _answer(false),
            child: Text(l.mobile_crew_approval_deny),
          ),
        ],
      ),
    );
  }
}
