import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/typing_dots.dart';

/// Three dots that rise in turn while the agent works.
class TypingBubble extends StatelessWidget {
  const TypingBubble({super.key});

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      decoration: BoxDecoration(
        color: c.base,
        border: Border.all(color: c.borderSubtle),
        borderRadius: BorderRadius.circular(18),
      ),
      child: TypingDots(color: c.muted, size: 6, gap: 2, stagger: .18),
    );
  }
}
