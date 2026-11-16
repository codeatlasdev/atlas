import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/git/sheets/sheet_frame.dart';

/// Asks for a pull request title and description: pops `(title, body)`, trimmed.
class PrSheet extends StatefulWidget {
  const PrSheet({super.key, required this.branch});
  final String branch;

  @override
  State<PrSheet> createState() => _PrSheetState();
}

class _PrSheetState extends State<PrSheet> {
  final _title = TextEditingController();
  final _body = TextEditingController();

  @override
  void dispose() {
    _title.dispose();
    _body.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => SheetFrame(
    title: 'Pull request from ${widget.branch}',
    children: [
      TextField(
        key: const ValueKey('pr-title'),
        controller: _title,
        onChanged: (_) => setState(() {}),
        decoration: const InputDecoration(
          labelText: 'Title',
          border: OutlineInputBorder(),
        ),
      ),
      const SizedBox(height: Space.sm),
      TextField(
        key: const ValueKey('pr-body'),
        controller: _body,
        minLines: 3,
        maxLines: 8,
        decoration: const InputDecoration(
          labelText: 'Description',
          border: OutlineInputBorder(),
        ),
      ),
      const SizedBox(height: Space.md),
      FilledButton(
        key: const ValueKey('pr-confirm'),
        onPressed: _title.text.trim().isEmpty
            ? null
            : () => Navigator.pop(context, (
                _title.text.trim(),
                _body.text.trim(),
              )),
        child: const Text('Open pull request'),
      ),
    ],
  );
}
