/// The pieces every "this crewmate wants to run a tool" surface shares (the chat bubble and
/// the Crew tab's needs-you card): the tool line, the command preview and Deny / Allow.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

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
    final text = Text(
      'Wants to run ${approval?.toolName ?? 'a tool'}',
      style: TextStyle(color: c.warning, fontSize: 13, fontWeight: weight),
    );
    return Row(
      children: [
        Icon(Icons.shield_outlined, size: 14, color: c.warning),
        const SizedBox(width: 4),
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
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
      decoration: BoxDecoration(
        color: c.ground,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Text(
        command,
        style: TextStyle(
          fontFamily: monoFamily,
          fontFamilyFallback: monoFallback,
          fontSize: 12,
          color: c.text,
        ),
      ),
    );
  }
}

/// Deny and Allow, right-aligned.
class ApprovalButtons extends StatelessWidget {
  const ApprovalButtons({
    super.key,
    required this.onAnswer,
    this.hPad = 14,
    this.vPad = 5,
    this.gap = 6,
  });

  final void Function(bool allow) onAnswer;
  final double hPad;
  final double vPad;
  final double gap;

  @override
  Widget build(BuildContext context) => Row(
    mainAxisAlignment: MainAxisAlignment.end,
    children: [
      OutlinePillButton(
        label: 'Deny',
        danger: true,
        hPad: hPad,
        vPad: vPad,
        onPressed: () => onAnswer(false),
      ),
      SizedBox(width: gap),
      WarnPillButton(
        label: 'Allow',
        hPad: hPad,
        vPad: vPad,
        onPressed: () => onAnswer(true),
      ),
    ],
  );
}
