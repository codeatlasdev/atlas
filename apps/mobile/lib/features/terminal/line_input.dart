import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// One line typed with the system keyboard, sent with Enter (the key bar covers Esc,
/// Tab, Ctrl and arrows).
class LineInput extends StatefulWidget {
  const LineInput({super.key, required this.onSubmit});
  final void Function(String) onSubmit;

  @override
  State<LineInput> createState() => _LineInputState();
}

class _LineInputState extends State<LineInput> {
  final _c = TextEditingController();

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  void _send() {
    widget.onSubmit(_c.text);
    _c.clear();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      color: c.base,
      padding: EdgeInsets.fromLTRB(
        Space.md,
        Space.sm,
        Space.sm,
        // The home indicator, unless the keyboard is up and already below us.
        MediaQuery.viewInsetsOf(context).bottom > 0
            ? Space.sm
            : MediaQuery.paddingOf(context).bottom,
      ),
      child: Row(
        children: [
          Expanded(
            child: TextField(
              controller: _c,
              autocorrect: false,
              enableSuggestions: false,
              textCapitalization: TextCapitalization.none,
              style: Theme.of(context).textTheme.bodyLarge!.copyWith(
                fontFamily: monoFamily,
                fontFamilyFallback: monoFallback,
              ),
              decoration: const InputDecoration(
                hintText: 'Type, then return',
                // Dense is 40 dp; a thumb needs 48.
                isDense: false,
                constraints: BoxConstraints(minHeight: 48),
              ),
              onSubmitted: (_) => _send(),
              textInputAction: TextInputAction.send,
            ),
          ),
          IconButton(
            tooltip: 'Send line',
            icon: const Icon(Icons.keyboard_return_rounded),
            onPressed: _send,
          ),
        ],
      ),
    );
  }
}
