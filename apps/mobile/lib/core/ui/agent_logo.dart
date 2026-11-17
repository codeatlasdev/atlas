import 'package:material_ui/material_ui.dart';
import 'package:path_parsing/path_parsing.dart';

import 'package:atlas_mobile/core/protocol/agent_registry.dart';
import 'package:atlas_mobile/core/ui/agent_logo_data.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// Registry ids Atlas launches (`crates/atlas-core/data/agents.json`, via the generated
/// `agent_registry.g.dart`); each has a mark drawn below.
final knownAgents = [for (final a in builtinAgents) a.id];

/// Display name of a registry id.
String agentLabel(String id) => switch (id) {
  '' => 'Agent',
  _ => builtinAgentOf(id)?.label ?? id,
};

Path _parse(String d) {
  final p = Path();
  writeSvgPathDataToPath(d, _Proxy(p));
  return p;
}

class _Proxy extends PathProxy {
  _Proxy(this.p);
  final Path p;
  @override
  void moveTo(double x, double y) => p.moveTo(x, y);
  @override
  void lineTo(double x, double y) => p.lineTo(x, y);
  @override
  void cubicTo(
    double x1,
    double y1,
    double x2,
    double y2,
    double x3,
    double y3,
  ) => p.cubicTo(x1, y1, x2, y2, x3, y3);
  @override
  void close() => p.close();
}

final _claude = _parse(claudeCodePath);
final _codexDark = _parse(codexWhitePath);
final _omp = _parse(ompGlyphPath);
final _opencodeInner = _parse(opencodeInnerPath);
final _opencodeFrame = _parse(opencodeFramePath)
  ..fillType = PathFillType.evenOdd;

/// The official mark of agent [agent] (a registry id), unaltered: omp's gradient glyph on
/// its dark tile, Claude Code's `#D97757`, Codex in white on this dark UI, OpenCode's frame on
/// its dark tile. An unknown
/// agent gets the generic icon.
class AgentLogo extends StatelessWidget {
  const AgentLogo(this.agent, {super.key, this.size = 24});

  final String agent;
  final double size;

  @override
  Widget build(BuildContext context) {
    if (!knownAgents.contains(agent)) {
      final c = AtlasColors.of(context);
      return SizedBox.square(
        dimension: size,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: c.raised,
            borderRadius: BorderRadius.circular(size * .22),
          ),
          child: Icon(
            Icons.smart_toy_outlined,
            size: size * .6,
            color: c.muted,
          ),
        ),
      );
    }
    return Semantics(
      label: agentLabel(agent),
      image: true,
      child: SizedBox.square(
        dimension: size,
        child: CustomPaint(painter: _AgentPainter(agent)),
      ),
    );
  }
}

class _AgentPainter extends CustomPainter {
  _AgentPainter(this.agent);
  final String agent;

  @override
  void paint(Canvas canvas, Size size) {
    switch (agent) {
      case 'omp':
        final s = size.width / 64;
        canvas.save();
        canvas.scale(s);
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            const Rect.fromLTWH(0, 0, 64, 64),
            const Radius.circular(12),
          ),
          Paint()..color = const Color(0xFF0F0A14),
        );
        canvas.drawPath(
          _omp,
          Paint()
            ..shader = const LinearGradient(
              begin: Alignment.topLeft,
              end: Alignment.bottomRight,
              colors: [Color(0xFFED4ABF), Color(0xFF9B4DFF), Color(0xFF5AD8E6)],
              stops: [0, .5, 1],
            ).createShader(const Rect.fromLTWH(0, 0, 64, 64)),
        );
        canvas.restore();
      case 'claude':
        canvas.scale(size.width / 24);
        canvas.drawPath(_claude, Paint()..color = const Color(0xFFD97757));
      case 'codex':
        canvas.scale(size.width / 24);
        // The app is dark only: the pair's dark-surface mark.
        canvas.drawPath(_codexDark, Paint()..color = const Color(0xFFFFFFFF));
      case 'opencode':
        canvas.scale(size.width / 512);
        canvas.drawRect(
          const Rect.fromLTWH(0, 0, 512, 512),
          Paint()..color = const Color(0xFF131010),
        );
        canvas.drawPath(
          _opencodeInner,
          Paint()..color = const Color(0xFF5A5858),
        );
        canvas.drawPath(
          _opencodeFrame,
          Paint()..color = const Color(0xFFFFFFFF),
        );
    }
  }

  @override
  bool shouldRepaint(_AgentPainter old) => old.agent != agent;
}
