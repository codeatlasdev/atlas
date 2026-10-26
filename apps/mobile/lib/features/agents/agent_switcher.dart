import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';

/// Every agent on the computer, the ones that need you first, to jump between them.
/// Pops with the chosen pane key.
Future<String?> showAgentSwitcher(
  BuildContext context, {
  required Map<String, AgentStatus> agents,
  required String current,
}) => showModalBottomSheet<String>(
  context: context,
  isScrollControlled: true,
  showDragHandle: true,
  backgroundColor: AtlasColors.of(context).overlay,
  builder: (_) => _Switcher(agents: agents, current: current),
);

class _Switcher extends StatelessWidget {
  const _Switcher({required this.agents, required this.current});
  final Map<String, AgentStatus> agents;
  final String current;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final entries = agents.entries.toList()
      ..sort((a, b) {
        final n = (needsYou(b.value) ? 1 : 0) - (needsYou(a.value) ? 1 : 0);
        return n != 0 ? n : b.value.updatedAt.compareTo(a.value.updatedAt);
      });
    return SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxHeight: MediaQuery.sizeOf(context).height * .7,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(
                Space.lg,
                0,
                Space.lg,
                Space.sm,
              ),
              child: Text(
                'Agents on this computer',
                style: Theme.of(context).textTheme.titleMedium,
              ),
            ),
            Flexible(
              child: ListView(
                shrinkWrap: true,
                padding: const EdgeInsets.only(bottom: Space.md),
                children: [
                  for (final e in entries)
                    _Row(
                      pane: e.key,
                      status: e.value,
                      selected: e.key == current,
                      color: c.agentState(e.value.state),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _Row extends StatelessWidget {
  const _Row({
    required this.pane,
    required this.status,
    required this.selected,
    required this.color,
  });

  final String pane;
  final AgentStatus status;
  final bool selected;
  final Color color;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return ListTile(
      selected: selected,
      selectedTileColor: c.hover,
      leading: AgentLogo(status.agent, size: 32),
      title: Text(
        agentTitle(status),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(fontWeight: FontWeight.w600),
      ),
      subtitle: Row(
        children: [
          StatusDot(
            color: color,
            pulse: status.state == 'working' || needsYou(status),
            size: 6,
          ),
          Flexible(
            child: Text(
              [
                agentStateLabel(status.state),
                agentLabel(status.agent),
              ].join(' · '),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: color, fontSize: 12),
            ),
          ),
        ],
      ),
      trailing: selected ? Icon(Icons.check, color: c.accent, size: 18) : null,
      onTap: () => Navigator.pop(context, pane),
    );
  }
}
