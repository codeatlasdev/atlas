import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/action/action_parts.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_card_frame.dart';

/// A title, a text field and Submit / Cancel: `provide_secret`, `input` and the fallback.
/// Give [reason] the action to show the agent's reason under the title.
class TextAction extends StatelessWidget {
  const TextAction({
    super.key,
    required this.title,
    required this.controller,
    required this.buttons,
    this.reason,
    this.hint,
    this.obscure = false,
    this.onSubmitted,
  });

  final String title;
  final TextEditingController controller;
  final Widget buttons;
  final TaskAction? reason;
  final String? hint;
  final bool obscure;
  final ValueChanged<String>? onSubmitted;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      ActionTitle(title),
      if (reason != null) ...actionReason(context, reason!),
      const SizedBox(height: Space.sm),
      AnswerField(
        controller: controller,
        hint: hint,
        obscure: obscure,
        onSubmitted: onSubmitted,
      ),
      const SizedBox(height: Space.sm),
      buttons,
    ],
  );
}
