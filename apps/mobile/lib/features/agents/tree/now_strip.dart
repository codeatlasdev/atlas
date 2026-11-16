import 'dart:async';

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/motion_loop.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';
import 'package:atlas_mobile/features/agents/tree/node_ring.dart';
import 'package:atlas_mobile/features/agents/tree/strip_decoration.dart';

/// One live line under the app bar: what the focused agent is doing right now, a ticking
/// timer, tokens and cost. A soft shimmer sweeps it while it runs.
class NowStrip extends StatefulWidget {
  const NowStrip({super.key, required this.node, required this.since});

  final AgentNode node;

  /// Epoch ms the focused agent started (state start for the root, derived for
  /// subagents); the timer counts from here while it runs.
  final int since;

  @override
  State<NowStrip> createState() => _NowStripState();
}

class _NowStripState extends State<NowStrip>
    with SingleTickerProviderStateMixin, MotionLoop {
  @override
  Duration get loopDuration => const Duration(milliseconds: 1800);

  @override
  bool get loopWhile => widget.node.running;

  /// Re-reads the clock each second so the elapsed time ticks while it runs.
  Timer? _tick;

  @override
  void syncLoop() {
    if (widget.node.running) {
      _tick ??= Timer.periodic(
        const Duration(seconds: 1),
        (_) => mounted ? setState(() {}) : null,
      );
    } else {
      _tick?.cancel();
      _tick = null;
    }
    super.syncLoop();
  }

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final n = widget.node;
    final running = n.running;
    final text = running
        ? (n.now ?? 'Thinking…')
        : switch (n.status) {
            'completed' => 'Finished',
            'failed' => 'Failed',
            'aborted' => 'Stopped',
            'pending' => 'Waiting to start',
            _ => 'Idle',
          };
    final dur = running ? elapsed(widget.since) : durationText(n.durationMs);
    final stats = statsLine(n);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        horizontal: Space.lg,
        vertical: Space.sm,
      ),
      decoration: stripDecoration(c),
      child: Row(
        children: [
          Expanded(
            flex: 2,
            child: AnimatedBuilder(
              animation: loop,
              builder: (_, child) => running && !Motion.reduced(context)
                  ? ShaderMask(
                      blendMode: BlendMode.srcIn,
                      shaderCallback: (r) => LinearGradient(
                        begin: Alignment(-1 + 3 * loop.value, 0),
                        end: Alignment(3 * loop.value, 0),
                        colors: [c.muted, c.text, c.muted],
                      ).createShader(r),
                      child: child,
                    )
                  : child!,
              child: Text(
                text,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodyMedium!
                    .copyWith(color: running ? c.text : c.muted),
              ),
            ),
          ),
          if (dur.isNotEmpty || stats.isNotEmpty) ...[
            const SizedBox(width: Space.sm),
            Flexible(
              child: Text(
                [
                  if (dur.isNotEmpty) dur,
                  if (stats.isNotEmpty) stats,
                ].join(' · '),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodySmall!.copyWith(
                  color: c.muted,
                  fontFeatures: const [FontFeature.tabularFigures()],
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}
