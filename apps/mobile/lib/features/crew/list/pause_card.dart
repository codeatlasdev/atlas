/// The Crew tab's kill switch for one crew server: a quiet "Pause all" on the tab's title line while it runs,
/// and a "Crew paused" banner with Resume while it is paused (running tasks were cancelled, queues are held,
/// automations are skipped).
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/attention_card.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

/// Pauses (`true`) or resumes (`false`) a crew server; throws when it refuses.
typedef SetPaused = Future<void> Function(bool paused);

/// Runs [onSet] once at a time and says why it failed; the two widgets below share it.
mixin _PauseAction<T extends StatefulWidget> on State<T> {
  bool busy = false;

  Future<void> setPaused(
    SetPaused onSet,
    String serverName,
    bool paused,
  ) async {
    setState(() => busy = true);
    try {
      await onSet(paused);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              "Couldn't ${paused ? 'pause' : 'resume'} $serverName: "
              '${e is StateError ? e.message : e}',
            ),
          ),
        );
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }
}

/// "Pause all" for a running crew server, sized for a title line.
class PauseAllButton extends StatefulWidget {
  const PauseAllButton({
    super.key,
    required this.serverName,
    required this.onSet,
  });

  final String serverName;
  final SetPaused onSet;

  @override
  State<PauseAllButton> createState() => _PauseAllButtonState();
}

class _PauseAllButtonState extends State<PauseAllButton>
    with _PauseAction<PauseAllButton> {
  @override
  Widget build(BuildContext context) => OutlinePillButton(
    label: 'Pause all',
    onPressed: busy
        ? null
        : () => setPaused(widget.onSet, widget.serverName, true),
    hPad: 12,
    vPad: 4,
    small: true,
  );
}

/// The banner of a paused crew server, with Resume.
class PausedBanner extends StatefulWidget {
  const PausedBanner({
    super.key,
    required this.serverName,
    required this.onSet,
    this.showServer = false,
  });

  final String serverName;
  final SetPaused onSet;

  /// Name the server (more than one crew server on the tab).
  final bool showServer;

  @override
  State<PausedBanner> createState() => _PausedBannerState();
}

class _PausedBannerState extends State<PausedBanner>
    with _PauseAction<PausedBanner> {
  @override
  Widget build(BuildContext context) => AttentionCard(
    margin: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, Space.md),
    icon: Icons.pause_circle_outline,
    title: 'Crew paused',
    body: widget.showServer
        ? '${widget.serverName}: nothing runs until you resume.'
        : 'Nothing runs until you resume. Queued tasks wait.',
    actions: [
      WarnPillButton(
        label: 'Resume',
        onPressed: busy
            ? null
            : () => setPaused(widget.onSet, widget.serverName, false),
      ),
    ],
  );
}
