import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// Copies [text] on long-press, with a haptic tick and a short confirmation.
Future<void> copyText(BuildContext context, String text) async {
  HapticFeedback.selectionClick();
  await Clipboard.setData(ClipboardData(text: text));
  if (!context.mounted) return;
  ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(
      const SnackBar(
        content: Text('Copied'),
        duration: Duration(milliseconds: 1200),
      ),
    );
}
