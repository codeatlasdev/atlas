import 'dart:convert';

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/agent_approval.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/attention_card.dart';

class ApprovalBanner extends StatelessWidget {
  const ApprovalBanner({
    super.key,
    required this.approval,
    required this.onAnswer,
  });
  final AgentApproval approval;
  final void Function(bool allow) onAnswer;

  @override
  Widget build(BuildContext context) {
    final input = approval.toolInput;
    final detail = input is Map && input['command'] is String
        ? input['command'] as String
        : const JsonEncoder.withIndent('  ').convert(input);
    return SizedBox(
      width: double.infinity,
      child: AttentionCard(
        strength: AttentionStrength.loud,
        radius: BorderRadius.circular(Space.radius),
        margin: const EdgeInsets.fromLTRB(12, 0, 12, 8),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(
                  Icons.shield_outlined,
                  size: 18,
                  color: AtlasColors.of(context).warning,
                ),
                const SizedBox(width: Space.sm),
                Expanded(
                  child: Text(
                    'Wants to run ${approval.toolName ?? 'a tool'}',
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                ),
              ],
            ),
            const SizedBox(height: Space.sm),
            // The command can be long: it scrolls inside a box that never takes more than
            // a quarter of the screen, so Deny and Allow stay in reach.
            ConstrainedBox(
              constraints: BoxConstraints(
                maxHeight: (MediaQuery.sizeOf(context).height * .25).clamp(
                  48.0,
                  120.0,
                ),
              ),
              child: SingleChildScrollView(
                child: SelectableText(
                  detail,
                  style: Theme.of(context).textTheme.bodySmall!.copyWith(
                    color: AtlasColors.of(context).text,
                    fontFamily: monoFamily,
                    fontFamilyFallback: monoFallback,
                  ),
                ),
              ),
            ),
            const SizedBox(height: Space.sm),
            Align(
              alignment: Alignment.centerRight,
              child: Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  OutlinedButton(
                    onPressed: () => onAnswer(false),
                    child: const Text('Deny'),
                  ),
                  FilledButton(
                    onPressed: () => onAnswer(true),
                    child: const Text('Allow'),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
