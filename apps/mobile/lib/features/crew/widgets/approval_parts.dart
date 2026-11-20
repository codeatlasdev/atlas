/// The pieces every "this crewmate wants to run a tool" surface shares (the chat bubble and
/// the Crew tab's needs-you card): the tool line, the command preview and the rail note. The answer buttons are
/// `permissions/approval_actions.dart`.
library;

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/permission_mode.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';

/// The shell command the tool wants to run, when its input has one.
String? approvalCommand(CrewmatePendingApproval? approval) {
  final input = approval?.toolInput;
  return input is Map && input['command'] is String
      ? input['command'] as String
      : null;
}

/// "Wants to run" plus the tool name, with the shield.
class ApprovalToolLine extends StatelessWidget {
  const ApprovalToolLine({
    super.key,
    required this.approval,
    required this.weight,
    this.flexible = false,
  });

  final CrewmatePendingApproval? approval;
  final FontWeight weight;

  /// Wrap the text in a [Flexible] so a long tool name ellipsizes instead of overflowing.
  final bool flexible;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final tool = approval?.toolName;
    final text = Text(
      tool == null
          ? l.mobile_crew_approval_wants_to_run_unknown
          : l.mobile_crew_approval_wants_to_run(tool),
      style: Theme.of(context).textTheme.bodyMedium!
          .copyWith(color: c.warning, fontWeight: weight),
    );
    return Row(
      children: [
        Icon(Icons.shield_outlined, size: 14, color: c.warning),
        const SizedBox(width: Space.xs),
        if (flexible) Flexible(child: text) else text,
      ],
    );
  }
}

/// The command, monospaced on the ground colour.
class ApprovalCommandPreview extends StatelessWidget {
  const ApprovalCommandPreview(this.command, {super.key});
  final String command;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        horizontal: Space.md,
        vertical: Space.sm,
      ),
      decoration: BoxDecoration(
        color: c.ground,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Text(
        command,
        style: Theme.of(context).textTheme.bodySmall!.copyWith(
          fontFamily: monoFamily,
          fontFamilyFallback: monoFallback,
          color: c.text,
        ),
      ),
    );
  }
}

/// Why a mode did not decide this call, when a rail held it: protected paths and tasks that never auto-run ask in
/// every mode, and can only be allowed once.
class ApprovalRailNote extends StatelessWidget {
  const ApprovalRailNote(this.rail, {super.key});
  final ApprovalRail rail;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(Icons.gpp_maybe_outlined, size: 14, color: c.muted),
        const SizedBox(width: Space.xs),
        Expanded(
          child: Text(
            switch (rail) {
              ApprovalRail.protectedPath =>
                l.mobile_crew_approval_rail_protected_path,
              ApprovalRail.noAuto => l.mobile_crew_approval_rail_no_auto,
            },
            style: Theme.of(context).textTheme.bodySmall!
                .copyWith(color: c.muted, height: 1.35),
          ),
        ),
      ],
    );
  }
}
