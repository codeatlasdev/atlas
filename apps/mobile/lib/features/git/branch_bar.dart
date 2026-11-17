import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/git/git_models.dart';

class BranchBar extends StatelessWidget {
  const BranchBar({super.key, required this.status});
  final GitStatus status;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Row(
      children: [
        Icon(Icons.call_split, size: 18, color: c.accent),
        const SizedBox(width: Space.sm),
        Flexible(
          child: Text(
            status.branch,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontWeight: FontWeight.w700),
          ),
        ),
        if (status.upstream != null) ...[
          const SizedBox(width: Space.sm),
          Text(
            '${status.ahead > 0 ? '↑${status.ahead} ' : ''}${status.upstream}',
            style: Theme.of(context).textTheme.bodySmall!
                .copyWith(color: c.muted),
          ),
        ] else ...[
          const SizedBox(width: Space.sm),
          Text(
            'not pushed',
            style: Theme.of(context).textTheme.bodySmall!
                .copyWith(color: c.muted),
          ),
        ],
      ],
    );
  }
}
