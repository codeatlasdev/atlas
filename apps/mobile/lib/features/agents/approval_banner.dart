import 'dart:convert';

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

class ApprovalBanner extends StatelessWidget {
  const ApprovalBanner({super.key, required this.approval, required this.onAnswer});
  final AgentApproval approval;
  final void Function(bool allow) onAnswer;

  @override
  Widget build(BuildContext context) {
    final input = approval.toolInput;
    final detail = input is Map && input['command'] is String
        ? input['command'] as String
        : const JsonEncoder.withIndent('  ').convert(input);
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(12, 0, 12, 8),
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: AtlasColors.of(context).warning.withValues(alpha: .12),
        border: Border.all(
          color: AtlasColors.of(context).warning.withValues(alpha: .6),
        ),
        borderRadius: BorderRadius.circular(Space.radius),
      ),
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
              const SizedBox(width: 6),
              Text(
                'Wants to run ${approval.toolName ?? 'a tool'}',
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
            ],
          ),
          const SizedBox(height: 6),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 120),
            child: SingleChildScrollView(
              child: SelectableText(
                detail,
                style: const TextStyle(
                  fontFamily: monoFamily,
                  fontFamilyFallback: monoFallback,
                  fontSize: 12,
                ),
              ),
            ),
          ),
          const SizedBox(height: 8),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              OutlinedButton(
                onPressed: () => onAnswer(false),
                child: const Text('Deny'),
              ),
              const SizedBox(width: 8),
              FilledButton(
                onPressed: () => onAnswer(true),
                child: const Text('Allow'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
