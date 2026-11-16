import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/git/sheets/sheet_frame.dart';

/// Asks for a commit message (typed, or written by the AI): pops it, trimmed.
class CommitSheet extends StatefulWidget {
  const CommitSheet({super.key, required this.staged, required this.suggest});
  final int staged;
  final Future<String?> Function() suggest;

  @override
  State<CommitSheet> createState() => _CommitSheetState();
}

class _CommitSheetState extends State<CommitSheet> {
  final _text = TextEditingController();
  bool _writing = false;

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  Future<void> _ai() async {
    setState(() => _writing = true);
    final m = await widget.suggest();
    if (!mounted) return;
    setState(() {
      _writing = false;
      if (m != null) _text.text = m;
    });
  }

  @override
  Widget build(BuildContext context) => SheetFrame(
    title:
        'Commit ${widget.staged} staged file${widget.staged == 1 ? '' : 's'}',
    children: [
      TextField(
        key: const ValueKey('commit-message'),
        controller: _text,
        minLines: 3,
        maxLines: 8,
        onChanged: (_) => setState(() {}),
        decoration: const InputDecoration(
          hintText: 'Subject, blank line, then why',
          border: OutlineInputBorder(),
        ),
      ),
      const SizedBox(height: Space.sm),
      Wrap(
        alignment: WrapAlignment.spaceBetween,
        crossAxisAlignment: WrapCrossAlignment.center,
        spacing: Space.sm,
        runSpacing: Space.sm,
        children: [
          OutlinedButton.icon(
            key: const ValueKey('ai-message'),
            onPressed: _writing ? null : _ai,
            icon: _writing
                ? const SizedBox(
                    width: Space.lg,
                    height: Space.lg,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.auto_awesome),
            label: Text(_writing ? 'Writing…' : 'Write with AI'),
          ),
          FilledButton(
            key: const ValueKey('commit-confirm'),
            onPressed: _text.text.trim().isEmpty
                ? null
                : () => Navigator.pop(context, _text.text.trim()),
            child: const Text('Commit'),
          ),
        ],
      ),
    ],
  );
}
