import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';

/// `292K/1M  29%`: how full the context window is.
class ContextBar extends StatelessWidget {
  const ContextBar({super.key, required this.tokens, required this.window});
  final int tokens;
  final int window;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final share = window <= 0 ? 0.0 : (tokens / window).clamp(0.0, 1.0);
    final pct = (share * 100).round();
    final color = share >= .9
        ? c.error
        : share >= .7
        ? c.warning
        : c.accent;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'context ${windowText(tokens)}/${windowText(window)}  $pct%',
          key: const ValueKey('context-label'),
          style: Theme.of(context).textTheme.bodySmall!
              .copyWith(color: c.muted),
        ),
        const SizedBox(height: Space.xs),
        ClipRRect(
          borderRadius: BorderRadius.circular(3),
          child: LinearProgressIndicator(
            key: const ValueKey('context-bar'),
            value: share,
            minHeight: 6,
            color: color,
            backgroundColor: c.borderSubtle,
          ),
        ),
      ],
    );
  }
}
