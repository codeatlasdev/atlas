import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// The row of keys a phone keyboard lacks: Esc, Tab, Ctrl (then A to Z) and the arrows.
class KeyBar extends StatefulWidget {
  const KeyBar({super.key, required this.onKey, required this.enabled});

  final void Function(String) onKey;
  final bool enabled;

  @override
  State<KeyBar> createState() => _KeyBarState();
}

class _KeyBarState extends State<KeyBar> {
  bool _ctrlActive = false;

  void _tap(String seq) {
    if (widget.enabled) widget.onKey(seq);
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final bg = c.base;

    Widget key(String label, String seq) => Padding(
      padding: const EdgeInsets.symmetric(horizontal: 3, vertical: 2),
      child: TextButton(
        style: TextButton.styleFrom(
          minimumSize: const Size(48, 48),
          padding: const EdgeInsets.symmetric(horizontal: Space.md),
          backgroundColor: c.raised,
          foregroundColor: c.text,
          disabledForegroundColor: c.faint,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(Space.radiusSm),
          ),
          textStyle: Theme.of(context).textTheme.bodyMedium!.copyWith(
            fontFamily: monoFamily,
            fontFamilyFallback: monoFallback,
          ),
        ),
        onPressed: widget.enabled ? () => _tap(seq) : null,
        child: Text(label),
      ),
    );

    final baseKeys = [
      key('Esc', '\x1b'),
      key('Tab', '\t'),
      // Ctrl toggle
      TextButton(
        style: TextButton.styleFrom(
          minimumSize: const Size(48, 48),
          padding: const EdgeInsets.symmetric(horizontal: Space.md),
          backgroundColor: _ctrlActive ? c.accent : c.raised,
          foregroundColor: _ctrlActive ? c.onAccent : c.text,
          disabledForegroundColor: c.faint,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(Space.radiusSm),
          ),
        ),
        onPressed: widget.enabled
            ? () => setState(() => _ctrlActive = !_ctrlActive)
            : null,
        child: const Text('Ctrl'),
      ),
      key('←', '\x1b[D'),
      key('↑', '\x1b[A'),
      key('↓', '\x1b[B'),
      key('→', '\x1b[C'),
    ];

    // When ctrl is active, show A–Z row instead of base keys.
    final ctrlKeys = [
      for (final c in 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split(''))
        key('^$c', String.fromCharCode(c.codeUnitAt(0) - 64)),
    ];

    return Container(
      decoration: BoxDecoration(
        color: bg,
        border: Border(top: BorderSide(color: c.borderSubtle)),
      ),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: Space.xs, vertical: 2),
        child: Row(children: _ctrlActive ? ctrlKeys : baseKeys),
      ),
    );
  }
}
