import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/attention_card.dart';

/// The frame of a card the crewmate raises in the thread (a question, an action to take):
/// a bubble as wide as a message, indented past the avatar column.
class CrewmateCardFrame extends StatelessWidget {
  const CrewmateCardFrame({super.key, required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Space.xs),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          const SizedBox(width: 36), // avatar placeholder alignment
          Flexible(
            child: AttentionCard(
              bubble: true,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: children,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// What went wrong with the last tap on a card (offline, refused): one line, under its body.
class CardError extends StatelessWidget {
  const CardError(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Text(
      text,
      style: Theme.of(context).textTheme.bodySmall!.copyWith(color: c.error),
    );
  }
}

/// The text field of a card that takes a typed answer.
class AnswerField extends StatelessWidget {
  const AnswerField({
    super.key,
    required this.controller,
    this.hint,
    this.obscure = false,
    this.onSubmitted,
  });

  final TextEditingController controller;
  final String? hint;
  final bool obscure;
  final ValueChanged<String>? onSubmitted;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final border = OutlineInputBorder(
      borderRadius: BorderRadius.circular(8),
      borderSide: BorderSide(color: c.borderSubtle),
    );
    return TextField(
      controller: controller,
      obscureText: obscure,
      style: Theme.of(context).textTheme.bodyLarge!.copyWith(color: c.text),
      decoration: InputDecoration(
        hintText: hint,
        hintStyle: Theme.of(context).textTheme.bodyLarge!
            .copyWith(color: c.faint),
        contentPadding: const EdgeInsets.symmetric(
          horizontal: Space.md,
          vertical: Space.sm,
        ),
        border: border,
        enabledBorder: border,
      ),
      onSubmitted: onSubmitted,
    );
  }
}
