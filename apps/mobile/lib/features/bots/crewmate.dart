/// The crewmate character, ported from Atlas Desktop (`apps/desktop/src/lib/bots/BotFace.svelte`,
/// `BotHat.svelte`, `look.ts`): the Atlas globe shell with a face and a hat, in the bot's own look.
/// It breathes, blinks, glances around, scans while working, hops when it needs you and sleeps
/// when its computer is offline. Same viewBox and coordinates as the desktop SVG.
library;

import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:path_parsing/path_parsing.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/bots/crew_shell_paths.dart';

// ── look (look.ts) ───────────────────────────────────────────────────────────

const crewColors = [
  Color(0xFF3D5BFF),
  Color(0xFF38BDF8),
  Color(0xFF8B5CF6),
  Color(0xFFEC4899),
  Color(0xFFF43F5E),
  Color(0xFFF97316),
  Color(0xFFEAB308),
  Color(0xFF22C55E),
  Color(0xFF14B8A6),
];

const _eyes = {'round', 'dots', 'happy', 'visor', 'cyclops', 'sleepy'};
const _hats = {
  'none',
  'straw',
  'cap',
  'beanie',
  'tophat',
  'crown',
  'party',
  'wizard',
  'headphones',
  'antenna',
  'bicorne',
  'bandana',
};

/// What a crewmate looks like. Built from the untrusted `look` of `bot_list`.
class CrewLook {
  const CrewLook({required this.color, required this.eyes, required this.hat});
  final Color color;
  final String eyes;
  final String hat;

  /// look.ts `hash`: the same seed picks the same colour as on the desktop.
  static int _hash(String seed) {
    var h = 0;
    for (final u in seed.codeUnits) {
      h = (h * 31 + u) & 0xFFFFFFFF;
    }
    return h;
  }

  /// look.ts `normalizeLook`: every valid field is kept, each invalid one falls back to the
  /// seed's default (a bad hat must not reset the colour).
  factory CrewLook.from(Map<String, dynamic>? raw, String seed) {
    final fallback = crewColors[_hash(seed) % crewColors.length];
    final hex = raw?['color'];
    final color = hex is String && RegExp(r'^#[0-9a-fA-F]{6}$').hasMatch(hex)
        ? Color(0xFF000000 | int.parse(hex.substring(1), radix: 16))
        : fallback;
    final eyes = raw?['eyes'];
    final hat = raw?['hat'];
    return CrewLook(
      color: color,
      eyes: _eyes.contains(eyes) ? eyes as String : 'round',
      hat: _hats.contains(hat) ? hat as String : 'straw',
    );
  }
}

/// look.ts `shade`: toward white (amount > 0) or black (amount < 0).
Color shade(Color c, double amount) {
  final a = amount.clamp(-1.0, 1.0);
  final t = a > 0 ? 1.0 : 0.0;
  double mix(double v) => v + (t - v) * a.abs();
  return Color.from(alpha: 1, red: mix(c.r), green: mix(c.g), blue: mix(c.b));
}

/// A bot's presence, as the face shows it.
enum CrewState {
  idle,
  working,
  needsYou,
  offline;

  static CrewState of(String state, {bool online = true}) {
    if (!online) return CrewState.offline;
    return switch (state) {
      'working' => CrewState.working,
      'needs_you' => CrewState.needsYou,
      _ => CrewState.idle,
    };
  }
}

// ── widget ───────────────────────────────────────────────────────────────────

class Crewmate extends StatefulWidget {
  const Crewmate({
    super.key,
    required this.seed,
    required this.look,
    required this.state,
    this.size = 48,
    this.animate = true,
  });

  /// Bot id: decides the colour when the bot never picked one, and offsets its motion.
  final String seed;
  final Map<String, dynamic>? look;
  final CrewState state;
  final double size;

  /// Off: drawn once at rest (rows in a long chat), no ticker.
  final bool animate;

  @override
  State<Crewmate> createState() => _CrewmateState();
}

class _CrewmateState extends State<Crewmate>
    with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker(_tick);
  late final math.Random _rand = math.Random(widget.seed.hashCode);
  late final double _phase = _rand.nextDouble() * 10;
  final _motion = ValueNotifier(_Pose.rest);

  Duration _now = Duration.zero;
  Duration _nextBlink = Duration.zero;
  Duration _blinkUntil = Duration.zero;
  Duration _nextGlance = Duration.zero;
  Offset _gaze = Offset.zero;
  Offset _gazeTarget = Offset.zero;
  double _kick = 0; // a squash on every state change, decaying
  CrewState? _lastState;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _syncTicker();
  }

  /// A list of still faces costs nothing; only the ones that matter move.
  void _syncTicker() {
    final still = Motion.reduced(context) || !widget.animate;
    if (still && _ticker.isActive) _ticker.stop();
    if (!still && !_ticker.isActive) _ticker.start();
  }

  @override
  void didUpdateWidget(Crewmate old) {
    super.didUpdateWidget(old);
    if (old.state != widget.state) _kick = 1;
    if (old.animate != widget.animate) _syncTicker();
  }

  void _tick(Duration now) {
    final dt = (now - _now).inMicroseconds / 1e6;
    _now = now;
    final t = now.inMicroseconds / 1e6 + _phase;
    final s = widget.state;
    _lastState ??= s;
    if (now >= _nextBlink && s != CrewState.offline) {
      _blinkUntil = now + const Duration(milliseconds: 130);
      _nextBlink = now + Duration(milliseconds: 1800 + _rand.nextInt(3800));
    }
    if (now >= _nextGlance) {
      if (s == CrewState.working) {
        _gazeTarget = Offset(_gazeTarget.dx > 0 ? -0.7 : 0.7, 0.25);
        _nextGlance = now + Duration(milliseconds: 700 + _rand.nextInt(500));
      } else if (s == CrewState.needsYou || _rand.nextDouble() < 0.35) {
        _gazeTarget = Offset.zero;
        _nextGlance = now + Duration(milliseconds: 1200 + _rand.nextInt(2600));
      } else {
        _gazeTarget = Offset(
          _rand.nextDouble() * 1.6 - 0.8,
          _rand.nextDouble() * 1.2 - 0.6,
        );
        _nextGlance = now + Duration(milliseconds: 1200 + _rand.nextInt(2600));
      }
    }
    // Pupils ease toward the target (the desktop's 160 ms transition).
    _gaze = Offset.lerp(_gaze, _gazeTarget, (dt * 12).clamp(0, 1))!;
    _kick = (_kick - dt * 3).clamp(0, 1);

    final (y, rot, sx, sy) = switch (s) {
      // A slow breath.
      CrewState.idle => (
        math.sin(t * 2) * 5,
        0.0,
        1.0,
        1.0 + math.sin(t * 2) * 0.012,
      ),
      // Busy: quicker bob and a side-to-side lean.
      CrewState.working => (
        -math.sin(t * 9).abs() * 10,
        math.sin(t * 4.5) * 4,
        1.0,
        1.0,
      ),
      // Hops for attention.
      CrewState.needsYou => (
        -math.pow(math.sin(t * 5).abs(), 2) * 34,
        0.0,
        1 + math.sin(t * 10) * 0.02,
        1 - math.sin(t * 10) * 0.02,
      ),
      CrewState.offline => (6.0, -4.0, 1.0, 1.0),
    };
    final k = math.sin(_kick * math.pi) * _kick;
    _motion.value = _Pose(
      y: y.toDouble(),
      rotation: rot,
      sx: sx + k * 0.08,
      sy: sy - k * 0.1,
      gaze: _gaze,
      blink: now < _blinkUntil,
      hatTip: math.sin(t * 1.3) * 2,
      time: t,
    );
  }

  @override
  void dispose() {
    _ticker.dispose();
    _motion.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final look = CrewLook.from(widget.look, widget.seed);
    return RepaintBoundary(
      child: CustomPaint(
        size: Size.square(widget.size),
        painter: _CrewPainter(
          look: look,
          state: widget.state,
          motion: _motion,
          detailed: widget.size >= 40,
          zColor: AtlasColors.of(context).muted,
        ),
      ),
    );
  }
}

class _Pose {
  const _Pose({
    required this.y,
    required this.rotation,
    required this.sx,
    required this.sy,
    required this.gaze,
    required this.blink,
    required this.hatTip,
    required this.time,
  });
  static const rest = _Pose(
    y: 0,
    rotation: 0,
    sx: 1,
    sy: 1,
    gaze: Offset.zero,
    blink: false,
    hatTip: 0,
    time: 0,
  );
  final double y, rotation, sx, sy, hatTip, time;
  final Offset gaze;
  final bool blink;
}

// ── painting (BotFace.svelte / BotHat.svelte) ────────────────────────────────

Path _p(String d) {
  final path = Path();
  writeSvgPathDataToPath(d, _Proxy(path));
  return path;
}

class _Proxy extends PathProxy {
  _Proxy(this.p);
  final Path p;
  @override
  void moveTo(double x, double y) => p.moveTo(x, y);
  @override
  void lineTo(double x, double y) => p.lineTo(x, y);
  @override
  void cubicTo(double a, double b, double c, double d, double e, double f) =>
      p.cubicTo(a, b, c, d, e, f);
  @override
  void close() => p.close();
}

final _shell = (() {
  final p = Path();
  for (final d in crewShellPaths) {
    p.addPath(_p(d), Offset.zero);
  }
  return p;
})();

const _ink = Color(0xFF14142B);
const _cheek = Color(0x59FF7AA8);
const _straw = Color(0xFFE8C36A);
const _strawEdge = Color(0xFFB8872E);

class _CrewPainter extends CustomPainter {
  _CrewPainter({
    required this.look,
    required this.state,
    required this.motion,
    required this.detailed,
    required this.zColor,
  }) : super(repaint: motion);

  final CrewLook look;
  final CrewState state;
  final ValueNotifier<_Pose> motion;
  final bool detailed;
  final Color zColor;

  static Paint fill(Color c) => Paint()..color = c;
  static Paint stroke(Color c, double w) => Paint()
    ..color = c
    ..style = PaintingStyle.stroke
    ..strokeWidth = w
    ..strokeCap = StrokeCap.round
    ..strokeJoin = StrokeJoin.round;

  @override
  void paint(Canvas canvas, Size size) {
    final p = motion.value;
    final asleep = state == CrewState.offline;
    // viewBox -90 -170 608 608
    canvas.scale(size.width / 608);
    canvas.translate(90, 170);

    // Ground shadow: shrinks as the body rises.
    final lift = 1 + p.y / 160;
    canvas.drawOval(
      Rect.fromCenter(
        center: const Offset(214, 432),
        width: 300 * lift,
        height: 32 * lift,
      ),
      fill(
        Colors.black.withValues(alpha: (0.22 * (1 + p.y / 120)).clamp(0, 1)),
      ),
    );

    canvas.save();
    canvas.translate(0, p.y);
    canvas.translate(214, 300);
    canvas.rotate(p.rotation * math.pi / 180);
    canvas.translate(-214, -300);
    canvas.translate(214, 426);
    canvas.scale(p.sx, p.sy);
    canvas.translate(-214, -426);

    if (asleep) {
      canvas.saveLayer(
        null,
        Paint()
          ..colorFilter = const ColorFilter.matrix(<double>[
            0.42, 0.5, 0.08, 0, 0, //
            0.32, 0.6, 0.08, 0, 0, //
            0.32, 0.5, 0.18, 0, 0, //
            0, 0, 0, 0.6, 0,
          ]),
      );
    }

    // Shell: radial gradient lit from the top left, then the globe tiles.
    canvas.drawCircle(
      const Offset(214, 213),
      206,
      Paint()
        ..shader =
            RadialGradient(
              center: const Alignment(-0.24, -0.4),
              radius: 0.75,
              colors: [
                shade(look.color, 0.35),
                look.color,
                shade(look.color, -0.35),
              ],
              stops: const [0, 0.55, 1],
            ).createShader(
              Rect.fromCircle(center: const Offset(214, 213), radius: 206),
            ),
    );
    if (detailed) {
      canvas.drawPath(
        _shell,
        fill(shade(look.color, 0.22).withValues(alpha: 0.55)),
      );
    }

    // Cheeks.
    for (final cx in [146.0, 282.0]) {
      canvas.drawOval(
        Rect.fromCenter(center: Offset(cx, 250), width: 52, height: 30),
        fill(_cheek),
      );
    }

    _eyesPaint(canvas, p, asleep);

    // Mouth.
    if (state == CrewState.needsYou) {
      canvas.drawOval(
        Rect.fromCenter(center: const Offset(214, 272), width: 28, height: 34),
        fill(_ink),
      );
    } else if (asleep) {
      canvas.drawLine(
        const Offset(198, 268),
        const Offset(230, 268),
        stroke(_ink, 10),
      );
    } else {
      canvas.drawPath(_p('M190 262 Q214 286 238 262'), stroke(_ink, 10));
    }

    canvas.save();
    canvas.translate(214, 60);
    canvas.rotate(p.hatTip * math.pi / 180);
    canvas.translate(-214, -60);
    _hat(canvas, p);
    canvas.restore();

    if (asleep) canvas.restore();
    canvas.restore();

    if (asleep) _zzz(canvas, p.time);
  }

  void _eyesPaint(Canvas canvas, _Pose p, bool asleep) {
    final px = p.gaze.dx * 14, py = p.gaze.dy * 10;
    canvas.save();
    if (p.blink) {
      canvas.translate(0, 196);
      canvas.scale(1, 0.08);
      canvas.translate(0, -196);
    }
    if (asleep) {
      canvas.drawPath(
        _p('M140 196 Q170 214 200 196 M228 196 Q258 214 288 196'),
        stroke(_ink, 12),
      );
    } else {
      switch (look.eyes) {
        case 'dots':
          for (final cx in [174.0, 254.0]) {
            canvas.drawOval(
              Rect.fromCenter(
                center: Offset(cx + px, 196 + py),
                width: 38,
                height: 52,
              ),
              fill(_ink),
            );
            canvas.drawCircle(
              Offset(cx + 6 + px, 186 + py),
              7,
              fill(Colors.white),
            );
          }
        case 'happy':
          canvas.save();
          canvas.translate(px * 0.6, py * 0.6);
          canvas.drawPath(
            _p('M144 206 Q172 166 200 206 M228 206 Q256 166 284 206'),
            stroke(_ink, 14),
          );
          canvas.restore();
        case 'visor':
          canvas.drawRRect(
            RRect.fromLTRBR(112, 160, 316, 232, const Radius.circular(36)),
            fill(_ink),
          );
          final glow = fill(shade(look.color, 0.6));
          for (final x in [150.0, 248.0]) {
            canvas.drawRRect(
              RRect.fromLTRBR(
                x + px * 1.6,
                178 + py * 0.8,
                x + 30 + px * 1.6,
                214 + py * 0.8,
                const Radius.circular(12),
              ),
              glow,
            );
          }
        case 'cyclops':
          canvas.drawCircle(const Offset(214, 192), 54, fill(Colors.white));
          canvas.drawCircle(
            Offset(214 + px * 1.4, 196 + py * 1.2),
            26,
            fill(_ink),
          );
          canvas.drawCircle(
            Offset(224 + px * 1.4, 184 + py * 1.2),
            8,
            fill(Colors.white),
          );
        default: // round, sleepy
          final lid = look.eyes == 'sleepy' ? 8.0 : 0.0;
          for (final cx in [170.0, 258.0]) {
            canvas.drawCircle(Offset(cx, 195), 36, fill(Colors.white));
            canvas.drawCircle(Offset(cx + px, 198 + py + lid), 18, fill(_ink));
            canvas.drawCircle(
              Offset(cx + 7 + px, 190 + py + lid),
              6,
              fill(Colors.white),
            );
          }
          if (look.eyes == 'sleepy') {
            canvas.drawPath(
              _p('M128 190 Q170 146 212 190 Z M216 190 Q258 146 300 190 Z'),
              fill(shade(look.color, -0.1)),
            );
          }
      }
    }
    canvas.restore();
  }

  void _hat(Canvas canvas, _Pose p) {
    final c = look.color;
    switch (look.hat) {
      case 'straw':
        canvas.save();
        canvas.translate(214, 40);
        canvas.rotate(-5 * math.pi / 180);
        canvas.translate(-214, -40);
        canvas.drawPath(_p('M84 56 Q214 -72 344 56 Z'), fill(_straw));
        final weave = stroke(_strawEdge.withValues(alpha: 0.42), 3.5);
        for (final d in [
          'M114 42 Q152 -4 174 -44',
          'M162 22 Q200 -20 214 -58',
          'M210 12 Q250 -24 260 -54',
        ]) {
          canvas.drawPath(_p(d), weave);
        }
        canvas.drawPath(
          _p('M214 -72 Q198 -24 196 56'),
          stroke(_strawEdge.withValues(alpha: 0.32), 2),
        );
        final brim = Rect.fromCenter(
          center: const Offset(214, 56),
          width: 356,
          height: 48,
        );
        canvas.drawOval(brim, fill(_straw));
        canvas.drawOval(brim, stroke(_strawEdge, 3.5));
        canvas.drawPath(_p('M84 56 Q214 -72 344 56'), stroke(_strawEdge, 3));
        canvas.drawPath(
          _p('M92 46 Q214 16 336 46'),
          stroke(c.withValues(alpha: 0.88), 11),
        );
        canvas.restore();
      case 'cap':
        canvas.drawPath(
          _p('M70 78 Q214 -96 358 78 Q214 46 70 78 Z'),
          fill(const Color(0xFFEF4444)),
        );
        canvas.drawPath(
          _p('M300 64 Q412 44 452 80 Q376 98 296 84 Z'),
          fill(const Color(0xFFB91C1C)),
        );
        canvas.drawCircle(
          const Offset(214, -10),
          12,
          fill(const Color(0xFFB91C1C)),
        );
      case 'beanie':
        canvas.drawPath(
          _p('M66 92 Q214 -120 362 92 Z'),
          fill(const Color(0xFFF59E0B)),
        );
        canvas.drawPath(
          _p('M58 96 Q214 40 370 96 L364 130 Q214 74 64 130 Z'),
          fill(const Color(0xFFD97706)),
        );
        canvas.drawCircle(
          Offset(214 + math.sin(p.time * 2.6) * 6, -22),
          30,
          fill(const Color(0xFFFDE68A)),
        );
      case 'tophat':
        canvas.save();
        canvas.translate(214, 40);
        canvas.rotate(-6 * math.pi / 180);
        canvas.translate(-214, -40);
        canvas.drawRRect(
          RRect.fromLTRBR(122, -132, 306, 38, const Radius.circular(14)),
          fill(const Color(0xFF18181B)),
        );
        canvas.drawRect(
          const Rect.fromLTRB(122, 2, 306, 28),
          fill(const Color(0xFFDC2626)),
        );
        canvas.drawOval(
          Rect.fromCenter(
            center: const Offset(214, 40),
            width: 344,
            height: 52,
          ),
          fill(const Color(0xFF27272A)),
        );
        canvas.restore();
      case 'crown':
        final crown = _p(
          'M112 56 L96 -58 L158 -4 L214 -86 L270 -4 L332 -58 L316 56 Q214 34 112 56 Z',
        );
        canvas.drawPath(crown, fill(const Color(0xFFFACC15)));
        canvas.drawPath(crown, stroke(const Color(0xFFCA8A04), 6));
        canvas.drawCircle(
          const Offset(214, 22),
          14,
          fill(const Color(0xFFEF4444)),
        );
        canvas.drawCircle(
          const Offset(150, 30),
          10,
          fill(const Color(0xFF22C55E)),
        );
        canvas.drawCircle(
          const Offset(278, 30),
          10,
          fill(const Color(0xFF3B82F6)),
        );
      case 'party':
        canvas.save();
        canvas.translate(214, 40);
        canvas.rotate(14 * math.pi / 180);
        canvas.translate(-214, -40);
        canvas.drawPath(
          _p('M146 44 L214 -146 L282 44 Q214 28 146 44 Z'),
          fill(const Color(0xFFEC4899)),
        );
        final band = fill(const Color(0xE6FDE047));
        canvas.drawPath(_p('M170 -24 L258 -24 L268 4 L160 4 Z'), band);
        canvas.drawPath(_p('M194 -86 L234 -86 L242 -64 L186 -64 Z'), band);
        canvas.drawCircle(
          const Offset(214, -150),
          18,
          fill(const Color(0xFFA5F3FC)),
        );
        canvas.restore();
      case 'wizard':
        canvas.drawPath(
          _p('M128 46 Q190 -40 228 -170 Q248 -60 300 46 Z'),
          fill(const Color(0xFF6D28D9)),
        );
        canvas.drawOval(
          Rect.fromCenter(
            center: const Offset(214, 46),
            width: 340,
            height: 52,
          ),
          fill(const Color(0xFF5B21B6)),
        );
        final tw = 0.9 + math.sin(p.time * 2.8) * 0.1;
        canvas.save();
        canvas.translate(214, -14);
        canvas.scale(tw);
        canvas.translate(-214, 14);
        canvas.drawPath(
          _p(
            'M214 -40 l9 20 21 3 -16 14 5 21 -19 -11 -19 11 5 -21 -16 -14 21 -3 Z',
          ),
          fill(const Color(0xFFFDE047)),
        );
        canvas.restore();
      case 'headphones':
        final glow = fill(shade(c, 0.55));
        canvas.drawPath(
          _p('M30 210 Q24 -44 214 -44 Q404 -44 398 210'),
          stroke(const Color(0xFF27272A), 28)..strokeCap = StrokeCap.butt,
        );
        final cup = fill(const Color(0xFF3F3F46));
        canvas.drawRRect(
          RRect.fromLTRBR(-14, 148, 50, 272, const Radius.circular(28)),
          cup,
        );
        canvas.drawRRect(
          RRect.fromLTRBR(378, 148, 442, 272, const Radius.circular(28)),
          cup,
        );
        canvas.drawRRect(
          RRect.fromLTRBR(4, 170, 20, 250, const Radius.circular(8)),
          glow,
        );
        canvas.drawRRect(
          RRect.fromLTRBR(408, 170, 424, 250, const Radius.circular(8)),
          glow,
        );
      case 'antenna':
        canvas.drawPath(
          _p('M214 14 Q210 -40 222 -92'),
          stroke(const Color(0xFF3F3F46), 12),
        );
        final beacon = 0.72 + math.sin(p.time * 3.9) * 0.28;
        canvas.drawCircle(
          const Offset(222, -100),
          24,
          fill(shade(c, 0.55).withValues(alpha: beacon)),
        );
      case 'bicorne':
        canvas.save();
        canvas.translate(214, 40);
        canvas.rotate(-4 * math.pi / 180);
        canvas.translate(-214, -40);
        final hat = _p(
          'M20 74 Q60 -40 214 -62 Q368 -40 408 74 Q300 30 214 34 Q128 30 20 74 Z',
        );
        canvas.drawPath(hat, fill(const Color(0xFFF1E4C2)));
        canvas.drawPath(hat, stroke(const Color(0xFFB89A5E), 5));
        canvas.drawLine(
          const Offset(214, -62),
          const Offset(214, 34),
          stroke(const Color(0xFFD8C08A), 4),
        );
        canvas.drawPath(
          _p('M70 40 Q120 -6 160 10 Q190 22 176 -20'),
          stroke(const Color(0x99B89A5E), 4),
        );
        canvas.drawPath(
          _p('M240 20 Q290 -30 330 0 Q350 14 372 -4'),
          stroke(const Color(0xFFE5484D), 5),
        );
        canvas.drawPath(
          _p('M362 -16 l20 20 M382 -16 l-20 20'),
          stroke(const Color(0xFFE5484D), 6),
        );
        canvas.drawPath(
          _p('M20 74 Q214 4 408 74'),
          stroke(c.withValues(alpha: 0.9), 10),
        );
        canvas.drawCircle(const Offset(214, -10), 20, fill(c));
        canvas.drawCircle(const Offset(214, -10), 20, stroke(Colors.white, 5));
        canvas.drawPath(
          _p('M214 -26 L220 -10 L214 6 L208 -10 Z'),
          fill(Colors.white),
        );
        canvas.restore();
      case 'bandana':
        final band = _p(
          'M40 110 Q50 -20 214 -26 Q378 -20 388 110 Q214 60 40 110 Z',
        );
        canvas.drawPath(band, fill(c));
        canvas.drawPath(band, fill(Colors.black.withValues(alpha: 0.18)));
        final dot = fill(Colors.white.withValues(alpha: 0.85));
        for (final (x, y, r) in [
          (120.0, 40.0, 9.0),
          (180.0, 10.0, 9.0),
          (250.0, 10.0, 9.0),
          (310.0, 40.0, 9.0),
          (214.0, 60.0, 9.0),
          (150.0, 70.0, 7.0),
          (280.0, 70.0, 7.0),
        ]) {
          canvas.drawCircle(Offset(x, y), r, dot);
        }
        final knot = fill(shade(c, -0.3));
        canvas.drawCircle(const Offset(392, 96), 20, knot);
        canvas.drawPath(
          _p('M392 96 l70 -10 l-20 40 Z M392 96 l54 50 l-42 8 Z'),
          knot,
        );
      default: // none
    }
  }

  void _zzz(Canvas canvas, double t) {
    for (final (i, x, y, s) in [
      (0, 390.0, 40.0, 70.0),
      (1, 440.0, -20.0, 54.0),
      (2, 480.0, -70.0, 40.0),
    ]) {
      final k = ((t / 3) + i * 0.3) % 1;
      final alpha = k < 0.2 ? k / 0.2 * 0.7 : 0.7 * (1 - (k - 0.2) / 0.8);
      final tp = TextPainter(
        text: TextSpan(
          text: 'z',
          style: TextStyle(
            fontSize: s,
            fontWeight: FontWeight.w700,
            color: zColor.withValues(alpha: alpha),
          ),
        ),
        textDirection: TextDirection.ltr,
      )..layout();
      tp.paint(canvas, Offset(x, y - s - k * 60));
    }
  }

  @override
  bool shouldRepaint(_CrewPainter old) =>
      old.look.color != look.color ||
      old.look.eyes != look.eyes ||
      old.look.hat != look.hat ||
      old.state != state ||
      old.detailed != detailed;
}
