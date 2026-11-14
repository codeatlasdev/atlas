/// The working row ("Reviewer is working, 12s") above the composer while a task runs.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/typing_dots.dart';
import 'package:atlas_mobile/features/crew/crew_text.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';

class CrewmateWorkingRow extends StatelessWidget {
  const CrewmateWorkingRow({
    super.key,
    required this.crewmate,
    required this.since,
  });
  final CrewmateEntry crewmate;
  final int? since;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final elapsed = since == null
        ? ''
        : ' · ${crewElapsed(since!, DateTime.now())}';
    return Padding(
      padding: const EdgeInsets.fromLTRB(Space.md, 0, Space.md, Space.xs),
      child: Row(
        children: [
          Crewmate(
            seed: crewmate.id,
            look: crewmate.look,
            state: CrewState.working,
            size: 30,
          ),
          const SizedBox(width: Space.sm),
          Text(
            '${crewmate.name} is working$elapsed',
            style: TextStyle(
              fontSize: 12,
              color: c.accent,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(width: 6),
          TypingDots(color: c.accent),
        ],
      ),
    );
  }
}
