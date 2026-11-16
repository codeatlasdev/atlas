/// The kinds that watch something: a GitHub repository, a path, another crewmate, a
/// service. Each edits [AutomationDraft] and calls `onChanged` to rebuild the sheet.
library;

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/automation_editor/automation_draft.dart';
import 'package:atlas_mobile/features/crew/automation_editor/field_label.dart';

const _githubEventOptions = [
  ('pr_opened', 'PR opened'),
  ('pr_review_requested', 'Review requested'),
  ('issue_opened', 'Issue opened'),
  ('ci_failed', 'CI fails'),
  ('release', 'Release published'),
  ('mention', 'You are mentioned'),
  ('issue_assigned', 'Issue assigned to you'),
  ('pr_review_comment', 'Review comment on a PR'),
];

class GithubFields extends StatelessWidget {
  const GithubFields({super.key, required this.draft, required this.onChanged});

  final AutomationDraft draft;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const FieldLabel('Repository (owner/repo)'),
        const SizedBox(height: Space.sm),
        TextField(
          controller: draft.githubRepo,
          decoration: const InputDecoration(
            hintText: 'owner/repo',
            isDense: true,
          ),
          onChanged: (_) => onChanged(),
        ),
        const SizedBox(height: Space.md),
        const FieldLabel('Events'),
        const SizedBox(height: Space.sm),
        Wrap(
          spacing: 4,
          runSpacing: 4,
          children: [
            for (final (key, label) in _githubEventOptions)
              FilterChip(
                label: Text(
                  label,
                  style: Theme.of(context).textTheme.bodySmall!
                      .copyWith(color: c.text),
                ),
                selected: draft.githubEvents.contains(key),
                onSelected: (on) {
                  on
                      ? draft.githubEvents.add(key)
                      : draft.githubEvents.remove(key);
                  onChanged();
                },
                materialTapTargetSize: MaterialTapTargetSize.padded,
              ),
          ],
        ),
      ],
    );
  }
}

class FileFields extends StatelessWidget {
  const FileFields({super.key, required this.draft, required this.onChanged});

  final AutomationDraft draft;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      const FieldLabel('Path'),
      const SizedBox(height: Space.sm),
      TextField(
        controller: draft.filePath,
        decoration: const InputDecoration(hintText: '/src', isDense: true),
        onChanged: (_) => onChanged(),
      ),
      const SizedBox(height: Space.md),
      const FieldLabel('Glob (optional)'),
      const SizedBox(height: Space.sm),
      TextField(
        controller: draft.fileGlob,
        decoration: const InputDecoration(hintText: '**/*.ts', isDense: true),
      ),
    ],
  );
}

class CrewmateFields extends StatelessWidget {
  const CrewmateFields({
    super.key,
    required this.draft,
    required this.onChanged,
  });

  final AutomationDraft draft;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) => _NameAndOutcome(
    label: 'Crewmate name',
    hint: 'Reviewer',
    controller: draft.fromCrewmate,
    onChanged: onChanged,
    outcomes: const [
      ButtonSegment(value: 'done', label: Text('When it finishes')),
      ButtonSegment(value: 'failed', label: Text('When it fails')),
    ],
    outcome: draft.crewmateOn,
    onOutcome: (v) => draft.crewmateOn = v,
  );
}

class ServiceFields extends StatelessWidget {
  const ServiceFields({
    super.key,
    required this.draft,
    required this.onChanged,
  });

  final AutomationDraft draft;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) => _NameAndOutcome(
    label: 'Service name',
    hint: 'api',
    controller: draft.service,
    onChanged: onChanged,
    outcomes: const [
      ButtonSegment(value: 'failed', label: Text('When it fails')),
      ButtonSegment(value: 'recovered', label: Text('When it recovers')),
    ],
    outcome: draft.serviceOn,
    onOutcome: (v) => draft.serviceOn = v,
  );
}

/// What a crewmate or a service trigger asks: a name, and which outcome of it to react to.
class _NameAndOutcome extends StatelessWidget {
  const _NameAndOutcome({
    required this.label,
    required this.hint,
    required this.controller,
    required this.onChanged,
    required this.outcomes,
    required this.outcome,
    required this.onOutcome,
  });

  final String label;
  final String hint;
  final TextEditingController controller;
  final VoidCallback onChanged;
  final List<ButtonSegment<String>> outcomes;
  final String outcome;
  final void Function(String outcome) onOutcome;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      FieldLabel(label),
      const SizedBox(height: Space.sm),
      TextField(
        controller: controller,
        decoration: InputDecoration(hintText: hint, isDense: true),
        onChanged: (_) => onChanged(),
      ),
      const SizedBox(height: Space.md),
      SegmentedButton<String>(
        segments: outcomes,
        selected: {outcome},
        onSelectionChanged: (s) {
          onOutcome(s.first);
          onChanged();
        },
        showSelectedIcon: false,
      ),
    ],
  );
}
