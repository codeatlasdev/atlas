/// The profile's modal prompts. Each returns what the user decided and does nothing else:
/// the screen owns the calls to the crew server.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// Edits the one-line job description; null when cancelled.
Future<String?> askRole(BuildContext context, String current) async {
  final controller = TextEditingController(text: current);
  final result = await showDialog<String>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: const Text('What it does'),
      content: TextField(
        controller: controller,
        maxLines: 4,
        autofocus: true,
        decoration: const InputDecoration(
          hintText: 'Describe what this crewmate does…',
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(ctx, controller.text.trim()),
          child: const Text('Save'),
        ),
      ],
    ),
  );
  controller.dispose();
  return result;
}

/// The memory file in a full-screen editor; null when closed without saving.
Future<String?> editMemory(BuildContext context, String text) async {
  final controller = TextEditingController(text: text);
  final saved = await showDialog<String>(
    context: context,
    builder: (ctx) => Dialog.fullscreen(
      child: Scaffold(
        appBar: AppBar(
          title: const Text('Memory'),
          leading: IconButton(
            icon: const Icon(Icons.close),
            onPressed: () => Navigator.pop(ctx),
          ),
          actions: [
            FilledButton(
              onPressed: () => Navigator.pop(ctx, controller.text),
              child: const Text('Save'),
            ),
            const SizedBox(width: Space.md),
          ],
        ),
        body: Padding(
          padding: const EdgeInsets.all(Space.lg),
          child: TextField(
            controller: controller,
            maxLines: null,
            expands: true,
            decoration: const InputDecoration(
              border: InputBorder.none,
              hintText: 'Memory contents…',
            ),
          ),
        ),
      ),
    ),
  );
  controller.dispose();
  return saved;
}

Future<bool> confirmDeleteCrewmate(BuildContext context) => _confirm(
  context,
  title: 'Delete crewmate?',
  message: 'This will permanently remove the crewmate and its history. This cannot be undone.',
  action: 'Delete',
);

Future<bool> confirmRemoveAutomation(BuildContext context, String what) =>
    _confirm(
      context,
      title: 'Remove automation?',
      message: 'Remove "$what"?',
      action: 'Remove',
    );

Future<bool> _confirm(
  BuildContext context, {
  required String title,
  required String message,
  required String action,
}) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: Text(title),
      content: Text(message),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(ctx, false),
          child: const Text('Cancel'),
        ),
        TextButton(
          onPressed: () => Navigator.pop(ctx, true),
          style: TextButton.styleFrom(
            foregroundColor: AtlasColors.of(ctx).error,
          ),
          child: Text(action),
        ),
      ],
    ),
  );
  return confirmed == true;
}
