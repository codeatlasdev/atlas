/// The Crew tab's logbook row: avatar, name and time, the crewmate's last word beneath.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_avatar.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/crew_text.dart';

class CrewmateRow extends StatelessWidget {
  const CrewmateRow({
    super.key,
    required this.entry,
    required this.showServer,
    required this.onTap,
    this.last = false,
  });

  final CrewEntry entry;
  final bool showServer;
  final VoidCallback onTap;

  /// The last row draws no divider under it.
  final bool last;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final crewmate = entry.crewmate;
    final state = CrewState.of(crewmate.state, online: entry.online);
    final when = crewTimeAgo(crewmate.last?.ts ?? 0, DateTime.now());
    final (preview, previewColor) = switch (state) {
      CrewState.working => ('working…', c.accent),
      CrewState.needsYou when crewmate.pendingApproval != null => (
        'waiting for your approval',
        c.warning,
      ),
      CrewState.needsYou => ('needs you', c.warning),
      _ => (crewPreview(crewmate.last?.text ?? ''), c.muted),
    };
    // Telegram's chat row: avatar, name and time on one line, the last message under
    // it, a hairline divider that starts after the avatar.
    return Material(
      type: MaterialType.transparency,
      child: InkWell(
        onTap: entry.online ? onTap : null,
        child: Padding(
          padding: const EdgeInsets.only(left: Space.lg),
          child: Row(
            children: [
              Crewmate(
                seed: crewmate.id,
                look: crewmate.look,
                state: state,
                size: 54,
                animate:
                    state == CrewState.working || state == CrewState.needsYou,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Container(
                  padding: const EdgeInsets.fromLTRB(0, 12, Space.lg, 12),
                  decoration: BoxDecoration(
                    border: last
                        ? null
                        : Border(bottom: BorderSide(color: c.borderSubtle)),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Expanded(
                            child: Text(
                              crewmate.name,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontWeight: FontWeight.w600,
                                fontSize: 16,
                                color: entry.online ? c.text : c.faint,
                              ),
                            ),
                          ),
                          if (when.isNotEmpty)
                            Text(
                              when,
                              style: TextStyle(fontSize: 13, color: c.faint),
                            ),
                        ],
                      ),
                      const SizedBox(height: 3),
                      Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Expanded(
                            child: Text(
                              preview,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 14.5,
                                height: 1.3,
                                color: previewColor,
                              ),
                            ),
                          ),
                          if (state == CrewState.needsYou) ...[
                            const SizedBox(width: 8),
                            Container(
                              margin: const EdgeInsets.only(top: 2),
                              width: 20,
                              height: 20,
                              decoration: BoxDecoration(
                                color: c.warning,
                                shape: BoxShape.circle,
                              ),
                              alignment: Alignment.center,
                              child: Text(
                                '!',
                                style: TextStyle(
                                  color: c.ground,
                                  fontSize: 12,
                                  fontWeight: FontWeight.w800,
                                ),
                              ),
                            ),
                          ],
                        ],
                      ),
                      if (showServer && entry.hostName.isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            entry.hostName,
                            style: TextStyle(fontSize: 12, color: c.faint),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
