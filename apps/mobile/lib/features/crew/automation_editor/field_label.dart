import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// The small caption above a field of the automation editor.
class FieldLabel extends StatelessWidget {
  const FieldLabel(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) => Text(
    text,
    style: Theme.of(context).textTheme.bodySmall!.copyWith(
      color: AtlasColors.of(context).muted,
      fontWeight: FontWeight.w500,
    ),
  );
}
