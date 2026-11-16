/// Bottom-sheet editor for creating or editing a crewmate automation trigger.
/// Returns a [Trigger] when the user taps Save, or null when cancelled.
///
/// The fields live in an [AutomationDraft]; `automation_editor/` holds the form of each
/// trigger kind.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_triggers.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/automation_editor/automation_draft.dart';
import 'package:atlas_mobile/features/crew/automation_editor/field_label.dart';
import 'package:atlas_mobile/features/crew/automation_editor/interval_fields.dart';
import 'package:atlas_mobile/features/crew/automation_editor/schedule_fields.dart';
import 'package:atlas_mobile/features/crew/automation_editor/source_fields.dart';

/// Show the automation editor sheet and return the resulting [Trigger], or null
/// if the user cancelled.  Pass [existing] to edit an existing trigger.
Future<Trigger?> showAutomationEditor(
  BuildContext context, {
  Trigger? existing,
  required String crewmateName,
}) {
  return showModalBottomSheet<Trigger>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    builder: (_) =>
        _AutomationEditorSheet(existing: existing, crewmateName: crewmateName),
  );
}

class _AutomationEditorSheet extends StatefulWidget {
  const _AutomationEditorSheet({this.existing, required this.crewmateName});
  final Trigger? existing;
  final String crewmateName;

  @override
  State<_AutomationEditorSheet> createState() => _AutomationEditorSheetState();
}

class _AutomationEditorSheetState extends State<_AutomationEditorSheet> {
  late final _draft = AutomationDraft(widget.existing);

  @override
  void dispose() {
    _draft.dispose();
    super.dispose();
  }

  void _refresh() => setState(() {});

  void _save() {
    if (!_draft.valid) return;
    Navigator.of(context).pop(_draft.toTrigger());
  }

  @override
  Widget build(BuildContext context) {
    final isNew = widget.existing == null;

    return Padding(
      padding: EdgeInsets.fromLTRB(
        Space.lg,
        0,
        Space.lg,
        Space.lg + MediaQuery.viewInsetsOf(context).bottom,
      ),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            // Header
            Padding(
              padding: const EdgeInsets.symmetric(vertical: Space.md),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      isNew ? 'New automation' : 'Edit automation',
                      style: Theme.of(context).textTheme.titleMedium,
                    ),
                  ),
                  TextButton(
                    onPressed: () => Navigator.of(context).pop(),
                    child: const Text('Cancel'),
                  ),
                ],
              ),
            ),

            // Kind picker
            const FieldLabel('Trigger'),
            const SizedBox(height: Space.sm),
            Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                for (final k in AutomationKind.values)
                  ChoiceChip(
                    label: Text(
                      k.label,
                      style: Theme.of(context).textTheme.bodySmall!
                          .copyWith(color: AtlasColors.of(context).text),
                    ),
                    selected: _draft.kind == k,
                    onSelected: (_) {
                      _draft.kind = k;
                      _refresh();
                    },
                    materialTapTargetSize: MaterialTapTargetSize.padded,
                  ),
              ],
            ),
            const SizedBox(height: Space.lg),

            // Kind-specific fields
            switch (_draft.kind) {
              AutomationKind.schedule => ScheduleFields(
                draft: _draft,
                onChanged: _refresh,
              ),
              AutomationKind.interval => IntervalFields(
                draft: _draft,
                onChanged: _refresh,
              ),
              AutomationKind.github => GithubFields(
                draft: _draft,
                onChanged: _refresh,
              ),
              AutomationKind.file => FileFields(
                draft: _draft,
                onChanged: _refresh,
              ),
              AutomationKind.crewmate => CrewmateFields(
                draft: _draft,
                onChanged: _refresh,
              ),
              AutomationKind.service => ServiceFields(
                draft: _draft,
                onChanged: _refresh,
              ),
            },

            const SizedBox(height: Space.lg),

            // Prompt
            const FieldLabel('Prompt'),
            const SizedBox(height: Space.sm),
            TextField(
              controller: _draft.prompt,
              minLines: 2,
              maxLines: 5,
              decoration: InputDecoration(
                hintText:
                    'What should ${widget.crewmateName} do when this fires?',
                isDense: true,
              ),
              onChanged: (_) => _refresh(),
            ),
            const SizedBox(height: Space.xl),

            // Save
            SizedBox(
              width: double.infinity,
              child: FilledButton(
                onPressed: _draft.valid ? _save : null,
                child: Text(isNew ? 'Add automation' : 'Save'),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
