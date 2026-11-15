/// The profile's Routines section: a crewmate's cron routines (a prompt it runs on a schedule), each with a
/// switch and a delete, and the sheet that adds or edits one. The list is saved whole
/// (`crewmate_routine_set`); hostd's cron parser has the last word and its message is shown when it refuses.
library;

import 'dart:math';

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/profile/profile_rows.dart';

class RoutinesSection extends StatelessWidget {
  const RoutinesSection({
    super.key,
    required this.crewmateName,
    required this.routines,
    required this.onAdd,
    required this.onEdit,
    required this.onToggle,
    required this.onDelete,
  });

  final String crewmateName;
  final List<Routine> routines;
  final VoidCallback onAdd;
  final void Function(Routine) onEdit;
  final void Function(Routine, bool) onToggle;
  final void Function(Routine) onDelete;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return ProfileSection(
      title: 'Routines',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (routines.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: Space.md),
              child: Text(
                'A routine gives $crewmateName a prompt to run on a cron schedule.',
                style: TextStyle(color: c.muted, fontSize: 13, height: 1.4),
              ),
            )
          else
            for (var i = 0; i < routines.length; i++)
              _RoutineRow(
                index: i,
                routine: routines[i],
                onEdit: () => onEdit(routines[i]),
                onToggle: (v) => onToggle(routines[i], v),
                onDelete: () => onDelete(routines[i]),
              ),
          const SizedBox(height: Space.sm),
          OutlinedButton.icon(
            onPressed: onAdd,
            icon: const Icon(Icons.add, size: 16),
            label: const Text('Add routine'),
          ),
        ],
      ),
    );
  }
}

class _RoutineRow extends StatelessWidget {
  const _RoutineRow({
    required this.index,
    required this.routine,
    required this.onEdit,
    required this.onToggle,
    required this.onDelete,
  });
  final int index;
  final Routine routine;
  final VoidCallback onEdit;
  final void Function(bool) onToggle;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return InkWell(
      onTap: onEdit,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: Space.sm),
        child: Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    routine.prompt,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      color: routine.enabled ? c.text : c.faint,
                      fontSize: 14,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    routine.cron,
                    style: TextStyle(
                      color: c.muted,
                      fontSize: 12,
                      fontFamily: monoFamily,
                      fontFamilyFallback: monoFallback,
                    ),
                  ),
                ],
              ),
            ),
            Switch(
              key: Key('routine-switch-$index'),
              value: routine.enabled,
              onChanged: onToggle,
            ),
            IconButton(
              key: Key('routine-delete-$index'),
              tooltip: 'Remove routine',
              icon: Icon(Icons.delete_outline_rounded, color: c.faint),
              onPressed: onDelete,
            ),
          ],
        ),
      ),
    );
  }
}

/// A fresh routine id: hostd only needs it unique within the crewmate.
String newRoutineId() =>
    'r-${DateTime.now().millisecondsSinceEpoch.toRadixString(36)}${Random().nextInt(1 << 16).toRadixString(36)}';

/// Shows the routine editor. [save] gets the routine to store and returns an error text to keep the sheet
/// open with (hostd's refusal), or null when saved. Returns whether it was saved.
Future<bool> showRoutineEditor(
  BuildContext context, {
  Routine? existing,
  required Future<String?> Function(Routine) save,
}) async {
  final saved = await showModalBottomSheet<bool>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    builder: (_) => _RoutineSheet(existing: existing, save: save),
  );
  return saved == true;
}

class _RoutineSheet extends StatefulWidget {
  const _RoutineSheet({this.existing, required this.save});
  final Routine? existing;
  final Future<String?> Function(Routine) save;

  @override
  State<_RoutineSheet> createState() => _RoutineSheetState();
}

class _RoutineSheetState extends State<_RoutineSheet> {
  late final _cron = TextEditingController(text: widget.existing?.cron ?? '');
  late final _prompt = TextEditingController(
    text: widget.existing?.prompt ?? '',
  );
  String? _error;
  bool _busy = false;

  @override
  void dispose() {
    _cron.dispose();
    _prompt.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final cron = _cron.text.trim();
    final prompt = _prompt.text.trim();
    if (!looksLikeCron(cron)) {
      setState(
        () => _error = 'A cron has five fields: minute hour day month weekday, like 30 2 * * *.',
      );
      return;
    }
    if (prompt.isEmpty) {
      setState(() => _error = 'What should it do when the routine fires?');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    final routine = widget.existing == null
        ? Routine.create(id: newRoutineId(), cron: cron, prompt: prompt)
        : widget.existing!.copyWith(cron: cron, prompt: prompt);
    final err = await widget.save(routine);
    if (!mounted) return;
    if (err == null) {
      Navigator.pop(context, true);
    } else {
      setState(() {
        _busy = false;
        _error = err;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: EdgeInsets.fromLTRB(
        Space.lg,
        Space.lg,
        Space.lg,
        Space.lg + MediaQuery.viewInsetsOf(context).bottom,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            widget.existing == null ? 'New routine' : 'Edit routine',
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: Space.md),
          TextField(
            key: const Key('routine-cron'),
            controller: _cron,
            autocorrect: false,
            enableSuggestions: false,
            style: const TextStyle(
              fontFamily: monoFamily,
              fontFamilyFallback: monoFallback,
            ),
            decoration: const InputDecoration(
              labelText: 'Cron',
              hintText: '30 2 * * *',
            ),
          ),
          const SizedBox(height: Space.md),
          TextField(
            key: const Key('routine-prompt'),
            controller: _prompt,
            maxLines: 3,
            minLines: 2,
            decoration: const InputDecoration(
              labelText: 'Prompt',
              hintText: 'What it should do',
            ),
          ),
          if (_error != null) ...[
            const SizedBox(height: Space.sm),
            Text(_error!, style: TextStyle(color: c.error, fontSize: 13)),
          ],
          const SizedBox(height: Space.md),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              TextButton(
                onPressed: _busy ? null : () => Navigator.pop(context, false),
                child: const Text('Cancel'),
              ),
              const SizedBox(width: Space.sm),
              FilledButton(
                onPressed: _busy ? null : _submit,
                child: const Text('Save'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
