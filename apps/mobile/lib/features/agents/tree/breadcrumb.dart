import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/tree/strip_decoration.dart';

/// `Main › Scout › Deep`, always on screen while a subagent is focused: every crumb but the
/// last is a chip that goes back up in one tap, `Main` first.
class Breadcrumb extends StatelessWidget {
  const Breadcrumb({super.key, required this.path, required this.onTap});

  final List<AgentNode> path;
  final void Function(String id) onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      width: double.infinity,
      decoration: stripDecoration(c),
      padding: const EdgeInsets.symmetric(horizontal: Space.md),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Row(
          children: [
            for (var i = 0; i < path.length; i++) ...[
              if (i > 0) Icon(Icons.chevron_right, size: 16, color: c.faint),
              _Crumb(
                key: ValueKey('crumb-${path[i].id}'),
                label: path[i].isRoot ? 'Main' : path[i].label,
                current: i == path.length - 1,
                home: path[i].isRoot,
                onTap: i == path.length - 1 ? null : () => onTap(path[i].id),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _Crumb extends StatelessWidget {
  const _Crumb({
    super.key,
    required this.label,
    required this.current,
    required this.home,
    required this.onTap,
  });
  final String label;
  final bool current;
  final bool home;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Material(
      color: current ? c.accent.withValues(alpha: .15) : c.raised,
      shape: StadiumBorder(
        side: BorderSide(
          color: current ? c.accent.withValues(alpha: .5) : c.borderSubtle,
        ),
      ),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        child: MinTapTarget(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: Space.md),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (home) ...[
                  Icon(Icons.arrow_back_rounded, size: 13, color: c.accent),
                  const SizedBox(width: Space.xs),
                ],
                Text(
                  label,
                  style: Theme.of(context).textTheme.bodySmall!.copyWith(
                    fontWeight: current || home
                        ? FontWeight.w700
                        : FontWeight.w500,
                    color: current
                        ? c.text
                        : home
                        ? c.accent
                        : c.muted,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Main › … › the node [focus]. When the tree does not list it (yet, or at all on an older
/// hostd) the path is built from its dotted id, so Main is always one tap away.
List<AgentNode> breadcrumbPath(AgentTree tree, String focus) {
  final listed = tree.path(focus);
  if (listed.isNotEmpty && listed.first.isRoot) return listed;
  final segs = focus.split('.');
  return [
    tree.node('') ?? const AgentNode(id: '', status: 'idle'),
    for (var i = 0; i < segs.length; i++)
      tree.node(segs.sublist(0, i + 1).join('.')) ??
          AgentNode(
            id: segs.sublist(0, i + 1).join('.'),
            parent: i == 0 ? '' : segs.sublist(0, i).join('.'),
            name: segs[i],
            status: 'pending',
          ),
  ];
}
