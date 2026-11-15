import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// omp's slash commands the phone can send; `/goal` needs text.
const _ompCommands = <(String, String, bool)>[
  ('/compact', 'Compact the context', false),
  ('/new', 'Start a new session', false),
  ('/goal', 'Set a goal for the agent', true),
  ('/export', 'Export the session', false),
];

/// The sheet of omp's slash commands: the picked command and whether it needs text, or
/// `null` when dismissed.
Future<(String, bool)?> showOmpCommands(BuildContext context) =>
    showModalBottomSheet<(String, bool)>(
      context: context,
      showDragHandle: true,
      backgroundColor: AtlasColors.of(context).overlay,
      builder: (ctx) {
        final c = AtlasColors.of(ctx);
        return SafeArea(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, Space.lg),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('Commands', style: Theme.of(ctx).textTheme.titleMedium),
                const SizedBox(height: Space.md),
                Wrap(
                  spacing: Space.sm,
                  runSpacing: Space.sm,
                  children: [
                    for (final (cmd, hint, arg) in _ompCommands)
                      ActionChip(
                        label: Text(arg ? '$cmd …' : cmd),
                        tooltip: hint,
                        backgroundColor: c.base,
                        side: BorderSide(color: c.borderSubtle),
                        labelStyle: Theme.of(context).textTheme.bodyMedium!
                            .copyWith(
                              fontFamily: monoFamily,
                              fontFamilyFallback: monoFallback,
                            ),
                        onPressed: () => Navigator.pop(ctx, (cmd, arg)),
                      ),
                  ],
                ),
              ],
            ),
          ),
        );
      },
    );
