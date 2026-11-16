import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/tree/strip_decoration.dart';

/// "Waiting on 2 jobs": the main agent's running subagents with a tiny progress bar each
/// (how much of their context window is used, else just a pulse). Tap one to open it.
class JobsStrip extends StatelessWidget {
  const JobsStrip({super.key, required this.tree, required this.onOpen});

  final AgentTree tree;
  final void Function(String id) onOpen;

  @override
  Widget build(BuildContext context) {
    final jobs = [
      for (final n in tree.subs)
        if (n.running || n.status == 'pending') n,
    ];
    if (jobs.isEmpty) return const SizedBox.shrink();
    final c = AtlasColors.of(context);
    return Container(
      width: double.infinity,
      decoration: stripDecoration(c),
      padding: const EdgeInsets.fromLTRB(
        Space.lg,
        Space.sm,
        Space.lg,
        Space.sm,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Waiting on ${jobs.length} ${jobs.length == 1 ? 'job' : 'jobs'}',
            key: const ValueKey('jobs-title'),
            style: Theme.of(context).textTheme.bodySmall!
                .copyWith(fontWeight: FontWeight.w700, color: c.accent),
          ),
          for (final j in jobs.take(5))
            InkWell(
              key: ValueKey('job-${j.id}'),
              onTap: () => onOpen(j.id),
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 3),
                child: Row(
                  children: [
                    Expanded(
                      flex: 5,
                      child: Text(
                        [
                          j.id,
                          if (j.now != null && j.now!.isNotEmpty) j.now!,
                        ].join(' · '),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: Theme.of(context).textTheme.bodySmall!
                            .copyWith(color: c.muted),
                      ),
                    ),
                    const SizedBox(width: Space.sm),
                    Expanded(
                      flex: 2,
                      child: ClipRRect(
                        borderRadius: BorderRadius.circular(2),
                        child: LinearProgressIndicator(
                          value: j.contextShare,
                          minHeight: 3,
                          color: c.accent,
                          backgroundColor: c.borderSubtle,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          if (jobs.length > 5)
            Text(
              '+${jobs.length - 5} more',
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: c.muted),
            ),
        ],
      ),
    );
  }
}
