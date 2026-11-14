/// The Crew tab's kill switch for one crew server: a quiet "Pause all" while it runs, and a
/// "Crew paused" banner with Resume while it is paused (running tasks were cancelled, queues
/// are held, automations are skipped).
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

class PauseCard extends StatefulWidget {
  const PauseCard({
    super.key,
    required this.serverName,
    required this.paused,
    required this.onSet,
    this.showServer = false,
  });

  final String serverName;
  final bool paused;

  /// Pauses (`true`) or resumes (`false`) the server; throws when it refuses.
  final Future<void> Function(bool paused) onSet;

  /// Name the server (more than one crew server on the tab).
  final bool showServer;

  @override
  State<PauseCard> createState() => _PauseCardState();
}

class _PauseCardState extends State<PauseCard> {
  bool _busy = false;

  Future<void> _set(bool paused) async {
    setState(() => _busy = true);
    try {
      await widget.onSet(paused);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              "Couldn't ${paused ? 'pause' : 'resume'} ${widget.serverName}: "
              '${e is StateError ? e.message : e}',
            ),
          ),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    if (!widget.paused) {
      return Padding(
        padding: const EdgeInsets.fromLTRB(Space.lg, Space.sm, Space.lg, 0),
        child: Align(
          alignment: Alignment.centerRight,
          child: OutlinePillButton(
            label: widget.showServer
                ? 'Pause all · ${widget.serverName}'
                : 'Pause all',
            onPressed: _busy ? null : () => _set(true),
            hPad: 12,
            vPad: 4,
            fontSize: 12,
          ),
        ),
      );
    }
    return Container(
      margin: const EdgeInsets.fromLTRB(Space.lg, Space.sm, Space.lg, 0),
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: c.warning.withValues(alpha: .08),
        border: Border.all(color: c.warning.withValues(alpha: .35)),
        borderRadius: BorderRadius.circular(14),
      ),
      child: Row(
        children: [
          Icon(Icons.pause_circle_outline, color: c.warning, size: 22),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Crew paused',
                  style: TextStyle(
                    fontWeight: FontWeight.w700,
                    fontSize: 14,
                    color: c.text,
                  ),
                ),
                const SizedBox(height: 2),
                Text(
                  widget.showServer
                      ? '${widget.serverName}: nothing runs until you resume.'
                      : 'Nothing runs until you resume. Queued tasks wait.',
                  style: TextStyle(fontSize: 12, color: c.muted),
                ),
              ],
            ),
          ),
          const SizedBox(width: 10),
          WarnPillButton(
            label: 'Resume',
            onPressed: _busy ? null : () => _set(false),
          ),
        ],
      ),
    );
  }
}
