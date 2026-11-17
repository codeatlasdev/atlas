import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/automation_editor/automation_draft.dart';

/// The interval kind: a number of minutes, with the usual ones one tap away.
class IntervalFields extends StatelessWidget {
  const IntervalFields({
    super.key,
    required this.draft,
    required this.onChanged,
  });

  final AutomationDraft draft;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Wrap(
      crossAxisAlignment: WrapCrossAlignment.center,
      spacing: 8,
      runSpacing: 4,
      children: [
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            SizedBox(
              width: 80,
              child: TextField(
                controller: draft.intervalMinutes,
                keyboardType: TextInputType.number,
                decoration: InputDecoration(
                  hintText: '60',
                  hintStyle: TextStyle(color: c.faint),
                  isDense: true,
                ),
                onChanged: (_) => onChanged(),
              ),
            ),
            const SizedBox(width: Space.sm),
            Text('minutes', style: TextStyle(color: c.muted)),
          ],
        ),
        for (final m in [15, 30, 60, 120])
          ActionChip(
            label: Text(
              m < 60 ? '${m}m' : '${m ~/ 60}h',
              style: Theme.of(context).textTheme.labelSmall!
                  .copyWith(color: c.text),
            ),
            onPressed: () {
              draft.intervalSecs = m * 60;
              draft.intervalMinutes.text = '$m';
              onChanged();
            },
            materialTapTargetSize: MaterialTapTargetSize.padded,
            padding: EdgeInsets.zero,
          ),
      ],
    );
  }
}
