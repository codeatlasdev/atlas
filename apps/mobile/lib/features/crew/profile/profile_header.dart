/// The top of a crewmate's profile: portrait, name, job tag and its state / engine / server chips.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
import 'package:atlas_mobile/features/crew/crew_text.dart';

class ProfileHeader extends StatelessWidget {
  const ProfileHeader({
    super.key,
    required this.crewmate,
    required this.hostName,
    required this.state,
  });
  final CrewmateEntry crewmate;
  final String hostName;
  final CrewState state;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final tag = crewRoleTag(crewmate.role);
    final color = state.color(c);
    final word = state.word[0].toUpperCase() + state.word.substring(1);
    return Column(
      children: [
        Crewmate(
          seed: crewmate.id,
          look: crewmate.look,
          state: state,
          size: 150,
        ),
        const SizedBox(height: Space.sm),
        Text(
          crewmate.name,
          style: TextStyle(
            fontSize: 28,
            fontWeight: FontWeight.w700,
            color: c.text,
          ),
          textAlign: TextAlign.center,
        ),
        if (tag.isNotEmpty) ...[
          const SizedBox(height: 2),
          Text(tag, style: TextStyle(color: c.muted, fontSize: 15)),
        ],
        const SizedBox(height: Space.md),
        Wrap(
          alignment: WrapAlignment.center,
          spacing: 8,
          runSpacing: 8,
          children: [
            ProfileChip(
              dot: color,
              label: word,
              color: color,
              pulse: state.pulses,
            ),
            ProfileChip(
              label: crewmate.agent,
              child: AgentLogo(crewmate.agent, size: 14),
            ),
            ProfileChip(icon: Icons.dns_outlined, label: hostName),
          ],
        ),
      ],
    );
  }
}

class ProfileChip extends StatelessWidget {
  const ProfileChip({
    super.key,
    required this.label,
    this.icon,
    this.child,
    this.dot,
    this.color,
    this.pulse = false,
  });
  final String label;
  final IconData? icon;
  final Widget? child;
  final Color? dot;
  final Color? color;
  final bool pulse;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
      decoration: BoxDecoration(
        color: (color ?? c.text).withValues(alpha: .08),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (dot != null) StatusDot(color: dot!, size: 6, pulse: pulse),
          if (icon != null) ...[
            Icon(icon, size: 14, color: c.muted),
            const SizedBox(width: 6),
          ],
          if (child != null) ...[child!, const SizedBox(width: 6)],
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 180),
            child: Text(
              label,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: color ?? c.muted,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
