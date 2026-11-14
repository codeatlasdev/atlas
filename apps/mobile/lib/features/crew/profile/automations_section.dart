/// The profile's Automations section: one swipe-to-delete row per trigger and an Add button.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/automations.dart';
import 'package:atlas_mobile/features/crew/profile/profile_rows.dart';

class AutomationsSection extends StatelessWidget {
  const AutomationsSection({
    super.key,
    required this.crewmate,
    required this.now,
    required this.onAdd,
    required this.onEdit,
    required this.onDelete,
    required this.onToggle,
  });

  final CrewmateEntry crewmate;
  final DateTime now;
  final VoidCallback onAdd;
  final void Function(Trigger) onEdit;
  final void Function(Trigger) onDelete;
  final void Function(Trigger, bool) onToggle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return ProfileSection(
      title: 'Automations',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (crewmate.triggers.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: Space.md),
              child: Text(
                'Or just ask ${crewmate.name}: "every weekday at 9, review the open PRs".',
                style: TextStyle(color: c.faint, fontSize: 13, height: 1.4),
              ),
            )
          else
            for (final t in crewmate.triggers)
              TriggerRow(
                trigger: t,
                now: now,
                onEdit: () => onEdit(t),
                onDelete: () => onDelete(t),
                onToggle: (v) => onToggle(t, v),
              ),
          const SizedBox(height: Space.sm),
          OutlinedButton.icon(
            onPressed: onAdd,
            icon: const Icon(Icons.add, size: 16),
            label: const Text('Add automation'),
          ),
        ],
      ),
    );
  }
}

class TriggerRow extends StatelessWidget {
  const TriggerRow({
    super.key,
    required this.trigger,
    required this.now,
    required this.onEdit,
    required this.onDelete,
    required this.onToggle,
  });

  final Trigger trigger;
  final DateTime now;
  final VoidCallback onEdit;
  final VoidCallback onDelete;
  final void Function(bool) onToggle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final desc = describeTrigger(trigger.kind, tz: trigger.tz);
    final nextStr = trigger.nextFire != null
        ? describeNext(trigger.nextFire!, now)
        : null;

    return Dismissible(
      key: ValueKey(trigger.id),
      direction: DismissDirection.endToStart,
      background: Container(
        alignment: Alignment.centerRight,
        padding: const EdgeInsets.only(right: Space.lg),
        color: c.error.withValues(alpha: .15),
        child: Icon(Icons.delete_outline, color: c.error, size: 20),
      ),
      confirmDismiss: (_) async {
        onDelete();
        return false; // actual removal is handled via _saveTriggers
      },
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: Space.xs),
        child: Row(
          children: [
            Expanded(
              child: InkWell(
                onTap: onEdit,
                borderRadius: BorderRadius.circular(Space.radiusSm),
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 4),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(desc, style: TextStyle(color: c.text, fontSize: 14)),
                      if (nextStr != null) ...[
                        const SizedBox(height: 2),
                        Text(
                          'Next: $nextStr',
                          style: TextStyle(color: c.faint, fontSize: 12),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
            ),
            Switch(
              value: trigger.enabled,
              onChanged: onToggle,
              activeThumbColor: c.accent,
            ),
            IconButton(
              icon: Icon(Icons.edit_outlined, size: 16, color: c.faint),
              onPressed: onEdit,
              tooltip: 'Edit',
            ),
          ],
        ),
      ),
    );
  }
}
