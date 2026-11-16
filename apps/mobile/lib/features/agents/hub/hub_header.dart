import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/hub/hub_filter.dart';
import 'package:atlas_mobile/features/agents/hub/hub_totals.dart';
import 'package:atlas_mobile/features/agents/tree/strip_decoration.dart';

class HubHeader extends StatelessWidget {
  const HubHeader({
    super.key,
    required this.running,
    required this.parked,
    required this.totals,
    required this.layout,
    required this.onLayout,
    required this.filter,
    required this.onFilter,
    required this.tree,
  });

  final int running;
  final int parked;
  final HubTotals totals;
  final HubLayout layout;
  final void Function(HubLayout) onLayout;
  final HubFilter filter;
  final void Function(HubFilter) onFilter;
  final AgentTree tree;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    String label(HubFilter f) => switch (f) {
      HubFilter.all => 'All ${tree.nodes.length}',
      HubFilter.running => 'Running ${tree.nodes.where(f.admits).length}',
      HubFilter.done => 'Done ${tree.nodes.where((n) => n.done).length}',
      HubFilter.failed => 'Failed ${tree.nodes.where((n) => n.failed).length}',
    };
    return Container(
      decoration: stripDecoration(c),
      padding: const EdgeInsets.fromLTRB(
        Space.lg,
        Space.sm,
        Space.lg,
        Space.md,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            alignment: WrapAlignment.spaceBetween,
            crossAxisAlignment: WrapCrossAlignment.center,
            spacing: Space.md,
            runSpacing: Space.sm,
            children: [
              Text(
                '$running running · $parked parked',
                key: const ValueKey('hub-counts'),
                style: Theme.of(context).textTheme.bodyLarge!
                    .copyWith(fontWeight: FontWeight.w700),
              ),
              SegmentedButton<HubLayout>(
                showSelectedIcon: false,
                style: const ButtonStyle(
                  tapTargetSize: MaterialTapTargetSize.padded,
                ),
                segments: const [
                  ButtonSegment(value: HubLayout.flat, label: Text('Flat')),
                  ButtonSegment(
                    value: HubLayout.byParent,
                    label: Text('By parent'),
                  ),
                ],
                selected: {layout},
                onSelectionChanged: (s) => onLayout(s.first),
              ),
            ],
          ),
          if (totals.line.isNotEmpty) ...[
            const SizedBox(height: Space.xs),
            Text(
              totals.line,
              key: const ValueKey('hub-totals'),
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: c.muted),
            ),
          ],
          const SizedBox(height: Space.sm),
          // Four chips share the width: none may hide off the edge of a phone.
          Row(
            children: [
              for (final f in HubFilter.values)
                Expanded(
                  child: Padding(
                    padding: EdgeInsets.only(
                      right: f == HubFilter.values.last ? 0 : Space.xs,
                    ),
                    child: ChoiceChip(
                      key: ValueKey('hub-filter-${f.name}'),
                      label: SizedBox(
                        width: double.infinity,
                        child: Text(
                          label(f),
                          textAlign: TextAlign.center,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                      selected: filter == f,
                      showCheckmark: false,
                      labelPadding: EdgeInsets.zero,
                      padding: EdgeInsets.zero,
                      onSelected: (_) => onFilter(f),
                    ),
                  ),
                ),
            ],
          ),
        ],
      ),
    );
  }
}
