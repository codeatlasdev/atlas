/// "New terminal": the two places a phone opens a shell on a computer (the project's page, the computer's page).
/// Both only choose among what the computer lists (projects, checkouts); `pty_open` takes ids, never a path.
library;

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/pty_open.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';
import 'package:atlas_mobile/features/projects/worktree.dart';

/// One choice in the sheet: a label, a secondary line and what it returns.
typedef _Choice<T> = ({String label, String? sub, T value});

Future<T?> _pick<T>(
  BuildContext context,
  String title,
  List<_Choice<T>> choices,
) => showModalBottomSheet<T>(
  context: context,
  showDragHandle: true,
  backgroundColor: AtlasColors.of(context).overlay,
  builder: (_) => SafeArea(
    child: ListView(
      shrinkWrap: true,
      padding: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, Space.lg),
      children: [
        Text(title, style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: Space.sm),
        for (final c in choices)
          ListTile(
            key: Key('new-terminal-${c.label}'),
            contentPadding: EdgeInsets.zero,
            title: Text(c.label),
            subtitle: c.sub == null ? null : Text(c.sub!),
            onTap: () => Navigator.pop(context, c.value),
          ),
      ],
    ),
  ),
);

/// The checkout to start in, asked only when [projectId] has more than one. `null` = cancelled; an empty record
/// (`worktree: null`) is the project's own folder.
Future<({String? worktree, String name})?> _pickCheckout(
  BuildContext context,
  WidgetRef ref,
  MacDevice device,
  String projectId,
  String projectName,
) async {
  List<Worktree> list;
  try {
    list = (await ref.read(
      worktreesProvider((device: device, projectId: projectId)).future,
    )).worktrees.where((w) => w.exists).toList();
  } catch (_) {
    // The list is a convenience: without it the project's own folder is still a valid, listed place.
    return (worktree: null, name: projectName);
  }
  if (list.length < 2 || !context.mounted) {
    return (worktree: null, name: projectName);
  }
  final picked = await _pick<Worktree>(context, 'Where', [
    for (final w in list)
      (label: w.main ? 'main' : w.name, sub: w.branch, value: w),
  ]);
  if (picked == null) return null;
  return (
    worktree: picked.main ? null : picked.id,
    name: picked.main ? projectName : picked.name,
  );
}

/// The project's page: a shell in this project, in the checkout the person picks.
Future<void> newTerminalInProject(
  BuildContext context,
  WidgetRef ref,
  MacDevice device,
  ProjectInfo project,
) async {
  final where = await _pickCheckout(
    context,
    ref,
    device,
    project.id,
    project.name,
  );
  if (where == null || !context.mounted) return;
  await openNewTerminal(
    context,
    ref,
    device,
    project: project.id,
    worktree: where.worktree,
    title: where.name,
  );
}

/// The computer's page: a crew server has no projects, so its terminal starts in the crew home; a computer asks
/// which project (then which checkout).
Future<void> newTerminalOnComputer(
  BuildContext context,
  WidgetRef ref,
  MacDevice device,
) async {
  if (isCrewServer(device)) {
    return openNewTerminal(context, ref, device, title: device.name);
  }
  final projects = await ref.read(projectListProvider(device).future);
  if (!context.mounted) return;
  if (projects.isEmpty) {
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('Add a project in Atlas first.')),
    );
    return;
  }
  final project = await _pick<ProjectInfo>(context, 'New terminal in', [
    for (final p in projects) (label: p.name, sub: null, value: p),
  ]);
  if (project == null || !context.mounted) return;
  await newTerminalInProject(context, ref, device, project);
}
