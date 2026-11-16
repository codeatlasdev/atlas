import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';

/// What the agent on this terminal is doing, from its hooks: state and the task
/// it is on, above its screen.
class AgentStrip extends ConsumerWidget {
  const AgentStrip({super.key, required this.device, required this.pane});
  final MacDevice device;
  final String pane;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final a = ref.watch(agentStatusMapNotifierProvider(device)).value?[pane];
    if (a == null) return const SizedBox.shrink();
    final c = AtlasColors.of(context);
    final k = c.agentState(a.state);
    final task = a.state == 'working' ? a.prompt : (a.lastMessage ?? a.prompt);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(
        Space.md,
        Space.sm,
        Space.md,
        Space.md,
      ),
      decoration: BoxDecoration(
        color: c.raised,
        border: Border(bottom: BorderSide(color: c.borderSubtle)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              StatusDot(color: k, pulse: a.state == 'working', size: 6),
              Text(
                agentStateLabel(a.state),
                style: Theme.of(context).textTheme.bodySmall!
                    .copyWith(color: k, fontWeight: FontWeight.w700),
              ),
            ],
          ),
          if (task != null && task.trim().isNotEmpty) ...[
            const SizedBox(height: 3),
            Text(
              task.replaceAll('\n', ' '),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context).textTheme.bodyMedium!
                  .copyWith(color: c.text, height: 1.3),
            ),
          ],
        ],
      ),
    );
  }
}
