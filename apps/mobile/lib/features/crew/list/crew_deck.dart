/// The Crew tab's deck: every crewmate standing together in a row, with the crew's state pill
/// and the Recruit slot.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';

class CrewDeck extends StatelessWidget {
  const CrewDeck({
    super.key,
    required this.entries,
    required this.onOpen,
    required this.onRecruit,
  });
  final List<CrewEntry> entries;
  final void Function(CrewEntry) onOpen;
  final VoidCallback? onRecruit;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final working = entries.where((e) => e.crewmate.state == 'working').length;
    final waiting = entries
        .where((e) => e.crewmate.state == 'needs_you')
        .length;
    final (label, color) = waiting > 0
        ? ('$waiting ${waiting == 1 ? 'needs' : 'need'} you', c.warning)
        : working > 0
        ? ('$working working', c.accent)
        : ('All hands on deck', c.success);
    return Stack(
      children: [
        Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(
                Space.lg,
                Space.md,
                Space.lg,
                0,
              ),
              child: Row(
                children: [
                  ShipFlagIcon(color: c.muted, accentColor: c.accent, size: 22),
                  const SizedBox(width: Space.sm),
                  Text(
                    '${entries.length} aboard',
                    style: TextStyle(
                      color: c.muted,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const Spacer(),
                  StatePill(
                    label: label,
                    color: color,
                    pulse: working + waiting > 0,
                  ),
                ],
              ),
            ),
            SizedBox(
              height: 168,
              child: ListView(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(horizontal: Space.md),
                children: [
                  for (var i = 0; i < entries.length; i++)
                    EnterFade(
                      index: i,
                      child: DeckMate(
                        entry: entries[i],
                        onTap: () => onOpen(entries[i]),
                      ),
                    ),
                  if (onRecruit != null) RecruitSlot(onTap: onRecruit!),
                ],
              ),
            ),
          ],
        ),
      ],
    );
  }
}

class DeckMate extends StatelessWidget {
  const DeckMate({super.key, required this.entry, required this.onTap});
  final CrewEntry entry;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final crewmate = entry.crewmate;
    final state = CrewState.of(crewmate.state, online: entry.online);
    return Semantics(
      button: true,
      label: '${crewmate.name}, ${state.word}',
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: entry.online ? onTap : null,
        child: SizedBox(
          width: 96,
          child: Column(
            children: [
              const SizedBox(height: Space.md),
              Crewmate(
                seed: crewmate.id,
                look: crewmate.look,
                state: state,
                size: 92,
              ),
              const SizedBox(height: 4),
              Text(
                crewmate.name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontWeight: FontWeight.w700,
                  color: entry.online ? c.text : c.faint,
                ),
              ),
              const SizedBox(height: 2),
              StateDot(state: state),
            ],
          ),
        ),
      ),
    );
  }
}

class RecruitSlot extends StatelessWidget {
  const RecruitSlot({super.key, required this.onTap});
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Semantics(
      button: true,
      label: 'Recruit a crewmate',
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: SizedBox(
          width: 96,
          child: Column(
            children: [
              const SizedBox(height: Space.md + 14),
              Container(
                width: 64,
                height: 64,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: c.accent.withValues(alpha: .08),
                  border: Border.all(
                    color: c.accent.withValues(alpha: .45),
                    width: 1.5,
                  ),
                ),
                child: Icon(Icons.add_rounded, color: c.accent, size: 30),
              ),
              const SizedBox(height: 18),
              Text(
                'Recruit',
                style: TextStyle(color: c.accent, fontWeight: FontWeight.w700),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class StateDot extends StatelessWidget {
  const StateDot({super.key, required this.state});
  final CrewState state;

  @override
  Widget build(BuildContext context) {
    final color = state.color(AtlasColors.of(context));
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        StatusDot(color: color, pulse: state.pulses, size: 6),
        Flexible(
          child: Text(
            state.word,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 11,
              color: color,
              fontWeight: FontWeight.w600,
            ),
          ),
        ),
      ],
    );
  }
}

class StatePill extends StatelessWidget {
  const StatePill({
    super.key,
    required this.label,
    required this.color,
    required this.pulse,
  });
  final String label;
  final Color color;
  final bool pulse;

  @override
  Widget build(BuildContext context) => AnimatedSwitcher(
    duration: Motion.normal,
    child: Container(
      key: ValueKey(label),
      padding: const EdgeInsets.fromLTRB(2, 3, 10, 3),
      decoration: BoxDecoration(
        color: color.withValues(alpha: .12),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          StatusDot(color: color, pulse: pulse, size: 6),
          Text(
            label,
            style: TextStyle(
              color: color,
              fontSize: 12,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    ),
  );
}
