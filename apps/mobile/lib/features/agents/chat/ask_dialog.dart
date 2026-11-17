import 'package:material_ui/material_ui.dart';

/// Asks for one line of text; `null` when dismissed.
Future<String?> askText(BuildContext context, String label) =>
    showDialog<String>(context: context, builder: (_) => AskDialog(label));

/// A one-field prompt; it owns its controller so the dialog's exit animation never
/// paints a disposed one.
class AskDialog extends StatefulWidget {
  const AskDialog(this.label, {super.key});
  final String label;

  @override
  State<AskDialog> createState() => _AskDialogState();
}

class _AskDialogState extends State<AskDialog> {
  final _ctl = TextEditingController();

  @override
  void dispose() {
    _ctl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.label),
    content: TextField(
      controller: _ctl,
      autofocus: true,
      minLines: 1,
      maxLines: 4,
      textCapitalization: TextCapitalization.sentences,
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () => Navigator.pop(context, _ctl.text.trim()),
        child: const Text('Send'),
      ),
    ],
  );
}
