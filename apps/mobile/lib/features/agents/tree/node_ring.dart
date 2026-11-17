import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/motion_loop.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';

Color nodeColor(AtlasColors c, AgentNode n) => switch (n.status) {
  'running' => c.accent,
  'completed' => c.success,
  'failed' || 'aborted' => c.error,
  _ => c.muted,
};

String nodeStatusLabel(AgentNode n) => switch (n.status) {
  'running' => 'running',
  'completed' => 'done',
  'failed' => 'failed',
  'aborted' => 'stopped',
  'idle' => 'idle',
  _ => 'queued',
};

/// One line of numbers: `48.2k tok · $0.31 · 12 tools`.
String statsLine(AgentNode n) => [
  if (n.tokens > 0) '${tokensText(n.tokens)} tok',
  if (costText(n.cost).isNotEmpty) costText(n.cost),
  if (n.toolCount > 0) '${n.toolCount} ${n.toolCount == 1 ? 'tool' : 'tools'}',
].join(' · ');

/// A node's avatar with its status ring: an arc that turns while it runs, a full ring
/// (green / red) when it ended, a dashed grey one while it waits to start.
class NodeRing extends StatefulWidget {
  const NodeRing({
    super.key,
    required this.node,
    required this.fallbackAgent,
    this.size = 34,
  });

  final AgentNode node;

  /// The root agent's id (`omp`): a subagent without its own `agent` shows its parent's.
  final String fallbackAgent;
  final double size;

  @override
  State<NodeRing> createState() => _NodeRingState();
}

class _NodeRingState extends State<NodeRing>
    with SingleTickerProviderStateMixin, MotionLoop {
  @override
  Duration get loopDuration => const Duration(milliseconds: 1100);

  @override
  bool get loopWhile => widget.node.running;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final n = widget.node;
    final s = widget.size;
    final agent = n.agent ?? widget.fallbackAgent;
    final badge = switch (n.status) {
      'completed' => (Icons.check_rounded, c.success),
      'failed' || 'aborted' => (Icons.close_rounded, c.error),
      _ => null,
    };
    return SizedBox.square(
      dimension: s + 8,
      child: Stack(
        alignment: Alignment.center,
        children: [
          AnimatedBuilder(
            animation: loop,
            builder: (_, _) => CustomPaint(
              size: Size.square(s + 8),
              painter: _RingPainter(
                color: nodeColor(c, n),
                status: n.status,
                turn: loop.value,
              ),
            ),
          ),
          if (agent.isNotEmpty && knownAgents.contains(agent))
            AgentLogo(agent, size: s - 8)
          else
            CircleAvatar(
              radius: (s - 8) / 2,
              backgroundColor: c.raised,
              child: Text(
                (n.label.isEmpty ? '·' : n.label[0]).toUpperCase(),
                style: TextStyle(
                  fontSize: s * .38,
                  fontWeight: FontWeight.w700,
                  color: c.text,
                ),
              ),
            ),
          if (badge != null)
            Positioned(
              right: 0,
              bottom: 0,
              child: Container(
                decoration: BoxDecoration(
                  color: c.ground,
                  shape: BoxShape.circle,
                ),
                padding: const EdgeInsets.all(1),
                child: Icon(badge.$1, size: 12, color: badge.$2),
              ),
            ),
        ],
      ),
    );
  }
}

class _RingPainter extends CustomPainter {
  _RingPainter({required this.color, required this.status, required this.turn});
  final Color color;
  final String status;
  final double turn;

  @override
  void paint(Canvas canvas, Size size) {
    final r = (size.shortestSide - 3) / 2;
    final rect = Rect.fromCircle(center: size.center(Offset.zero), radius: r);
    final p = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2.5
      ..strokeCap = StrokeCap.round
      ..color = color;
    switch (status) {
      case 'running':
        canvas.drawCircle(
          rect.center,
          r,
          Paint()
            ..style = PaintingStyle.stroke
            ..strokeWidth = 2.5
            ..color = color.withValues(alpha: .18),
        );
        canvas.drawArc(rect, turn * 2 * math.pi, math.pi * .7, false, p);
      case 'pending':
        // A dashed ring: not started.
        const dashes = 14;
        for (var i = 0; i < dashes; i++) {
          canvas.drawArc(
            rect,
            i * 2 * math.pi / dashes,
            math.pi / dashes,
            false,
            p,
          );
        }
      default:
        canvas.drawCircle(rect.center, r, p);
    }
  }

  @override
  bool shouldRepaint(_RingPainter old) =>
      old.color != color || old.status != status || old.turn != turn;
}
