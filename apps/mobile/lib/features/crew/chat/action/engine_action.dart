import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/action/action_parts.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

/// `sign_in_engine`: the engine's sign-in is opened from here ([onSignIn]) when hostd can
/// run it; the card's [buttons] (Done) say it is finished.
class SignInEngineAction extends StatelessWidget {
  const SignInEngineAction({
    super.key,
    required this.action,
    required this.busy,
    required this.onSignIn,
    required this.buttons,
  });

  final TaskAction action;
  final bool busy;
  final void Function(String engine) onSignIn;
  final Widget buttons;

  @override
  Widget build(BuildContext context) {
    final engine = action.engine ?? 'engine';
    // Only the engines hostd can sign in get the button; any other name keeps the Settings text.
    final canSignInHere = engineLabels.containsKey(engine);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        ActionTitle('Sign in to ${engineLabels[engine] ?? engine}'),
        const SizedBox(height: Space.xs),
        Text(
          canSignInHere
              ? 'Tap Sign in here to open its sign-in on the crew server, finish it, then tap Done.'
              : 'Sign in to $engine on the crew server, then tap Done.',
          style: Theme.of(context).textTheme.bodySmall!
              .copyWith(color: AtlasColors.of(context).muted),
        ),
        const SizedBox(height: Space.md),
        if (canSignInHere) ...[
          WarnPillButton(
            key: const Key('sign-in-here'),
            label: 'Sign in here',
            onPressed: busy ? null : () => onSignIn(engine),
          ),
          const SizedBox(height: Space.sm),
        ],
        buttons,
      ],
    );
  }
}
