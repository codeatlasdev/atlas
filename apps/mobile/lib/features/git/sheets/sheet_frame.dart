import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// The body of a git bottom sheet: a [title] and the form under it, lifted above the keyboard.
class SheetFrame extends StatelessWidget {
  const SheetFrame({super.key, required this.title, required this.children});

  final String title;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) => Padding(
    padding: EdgeInsets.fromLTRB(
      Space.lg,
      0,
      Space.lg,
      MediaQuery.viewInsetsOf(context).bottom + Space.lg,
    ),
    child: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(title, style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: Space.md),
        ...children,
      ],
    ),
  );
}
