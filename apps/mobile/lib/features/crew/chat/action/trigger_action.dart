import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/protocol/crew_triggers.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

/// `add_trigger`: the crewmate proposes an automation; the user adds it or skips.
class AddTriggerAction extends StatelessWidget {
  const AddTriggerAction({
    super.key,
    required this.action,
    required this.crewmateId,
    required this.busy,
    required this.onAdd,
    required this.onSkip,
  });

  final TaskAction action;
  final String crewmateId;
  final bool busy;
  final VoidCallback onAdd;
  final VoidCallback onSkip;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final triggerKind = action.triggerKind;
    final triggerPrompt = action.triggerPrompt;
    final resolved = action.reason;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(Icons.schedule_outlined, size: 14, color: c.warning),
            const SizedBox(width: Space.sm),
            Flexible(
              child: Text(
                '${action.name ?? crewmateId} wants to add an automation',
                style: Theme.of(context).textTheme.bodyMedium!
                    .copyWith(fontWeight: FontWeight.w600, color: c.warning),
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ],
        ),
        const SizedBox(height: Space.sm),
        if (triggerKind != null) ...[
          Text(
            _describeTriggerKind(triggerKind),
            style: Theme.of(context).textTheme.bodyMedium!
                .copyWith(color: c.text),
          ),
          const SizedBox(height: 2),
        ],
        if (triggerPrompt != null && triggerPrompt.isNotEmpty) ...[
          Text(
            triggerPrompt,
            style: Theme.of(context).textTheme.bodySmall!
                .copyWith(color: c.muted),
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
          ),
          const SizedBox(height: 2),
        ],
        if (resolved != null) ...[
          Text(
            resolved,
            style: Theme.of(context).textTheme.bodySmall!
                .copyWith(color: c.muted),
          ),
          const SizedBox(height: Space.sm),
        ] else
          const SizedBox(height: Space.sm),
        Row(
          children: [
            WarnPillButton(label: 'Add', onPressed: busy ? null : onAdd),
            const SizedBox(width: Space.sm),
            OutlinePillButton(
              label: 'Not now',
              onPressed: busy ? null : onSkip,
            ),
          ],
        ),
      ],
    );
  }
}

String _describeTriggerKind(TriggerKind kind) => switch (kind) {
  ScheduleTrigger(:final cron) => 'Schedule: $cron',
  IntervalTrigger(:final everySecs) => 'Every ${_formatInterval(everySecs)}',
  WebhookTrigger() => 'Webhook',
  GithubTrigger(:final repo, :final events) =>
    'GitHub: $repo (${events.join(', ')})',
  FileTrigger(:final path, :final glob) =>
    'File: $path${glob != null ? '/$glob' : ''}',
  CrewmateTrigger(:final fromCrewmate, :final on) =>
    'After $fromCrewmate ($on)',
  ServiceTrigger(:final service, :final on) => 'Service $service ($on)',
};

String _formatInterval(int secs) {
  if (secs < 60) return '${secs}s';
  if (secs < 3600) return '${secs ~/ 60}m';
  return '${secs ~/ 3600}h';
}
