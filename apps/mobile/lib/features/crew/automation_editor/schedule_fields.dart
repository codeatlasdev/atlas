import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/automation_editor/automation_draft.dart';
import 'package:atlas_mobile/features/crew/automation_editor/field_label.dart';

/// The schedule kind: weekdays and a time, or a raw cron expression. Edits go to [draft];
/// [onChanged] rebuilds the sheet.
class ScheduleFields extends StatelessWidget {
  const ScheduleFields({
    super.key,
    required this.draft,
    required this.onChanged,
  });

  final AutomationDraft draft;
  final VoidCallback onChanged;

  static const _presets = [
    ('Every day', [0, 1, 2, 3, 4, 5, 6]),
    ('Weekdays', [1, 2, 3, 4, 5]),
  ];

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    if (draft.useCustomCron) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const FieldLabel('Cron expression'),
          const SizedBox(height: Space.sm),
          Row(
            children: [
              Expanded(
                child: TextField(
                  controller: draft.customCron,
                  decoration: const InputDecoration(hintText: '0 9 * * 1-5'),
                  style: TextStyle(fontFamily: monoFamily, color: c.text),
                  onChanged: (_) => onChanged(),
                ),
              ),
              TextButton(
                onPressed: () {
                  draft.useCustomCron = false;
                  onChanged();
                },
                child: const Text('Builder'),
              ),
            ],
          ),
        ],
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const FieldLabel('Days'),
        const SizedBox(height: Space.sm),
        // Preset chips
        Wrap(
          spacing: 6,
          children: [
            for (final (label, days) in _presets)
              FilterChip(
                label: Text(
                  label,
                  style: Theme.of(context).textTheme.bodySmall!
                      .copyWith(color: c.text),
                ),
                selected:
                    draft.days.length == days.length &&
                    days.every(draft.days.contains),
                onSelected: (_) {
                  draft.days = List.of(days);
                  onChanged();
                },
              ),
          ],
        ),
        const SizedBox(height: Space.sm),
        // Weekday chips
        Wrap(
          spacing: 4,
          children: [
            for (int i = 0; i < 7; i++)
              FilterChip(
                label: Text(
                  ['Su', 'M', 'Tu', 'W', 'Th', 'F', 'Sa'][i],
                  style: Theme.of(context).textTheme.labelSmall!
                      .copyWith(color: c.text),
                ),
                selected: draft.days.contains(i),
                onSelected: (on) {
                  on ? draft.days.add(i) : draft.days.remove(i);
                  draft.days.sort();
                  onChanged();
                },
                materialTapTargetSize: MaterialTapTargetSize.padded,
                padding: EdgeInsets.zero,
              ),
          ],
        ),
        const SizedBox(height: Space.md),
        const FieldLabel('Time'),
        const SizedBox(height: Space.sm),
        Wrap(
          crossAxisAlignment: WrapCrossAlignment.center,
          spacing: 6,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                _TimeDropdown(
                  value: draft.hour,
                  max: 23,
                  onChanged: (v) {
                    draft.hour = v;
                    onChanged();
                  },
                ),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: Space.sm),
                  child: Text(
                    ':',
                    style: Theme.of(context).textTheme.titleMedium!
                        .copyWith(color: c.muted, fontWeight: FontWeight.w400),
                  ),
                ),
                _TimeDropdown(
                  value: draft.minute,
                  max: 59,
                  step: 5,
                  onChanged: (v) {
                    draft.minute = v;
                    onChanged();
                  },
                ),
              ],
            ),
            TextButton(
              onPressed: () {
                draft.useCustomCron = true;
                onChanged();
              },
              child: const Text('Custom cron'),
            ),
          ],
        ),
      ],
    );
  }
}

/// A two-digit number picker: 0 to [max] in steps of [step] (the current value is always
/// listed, even when it is off the steps).
class _TimeDropdown extends StatelessWidget {
  const _TimeDropdown({
    required this.value,
    required this.max,
    this.step = 1,
    required this.onChanged,
  });

  final int value;
  final int max;
  final int step;
  final void Function(int) onChanged;

  @override
  Widget build(BuildContext context) {
    final values = [for (int i = 0; i <= max; i += step) i];
    // If current value not in steps, add it
    if (!values.contains(value)) {
      values
        ..add(value)
        ..sort();
    }
    // The dropdown is a 24 dp line of text; the box around it is the 48 dp a thumb needs.
    return ConstrainedBox(
      constraints: const BoxConstraints(minHeight: 48, minWidth: 48),
      child: DropdownButtonHideUnderline(
        child: DropdownButton<int>(
          value: value,
          items: values
              .map(
                (v) => DropdownMenuItem(
                  value: v,
                  child: Text(v.toString().padLeft(2, '0')),
                ),
              )
              .toList(),
          onChanged: (v) => onChanged(v!),
          isDense: true,
        ),
      ),
    );
  }
}
