/// The crewmate character, ported from Atlas Desktop (`apps/desktop/src/lib/crew/CrewmateFace.svelte`,
/// `CrewmateHat.svelte`, `look.ts`): the Atlas globe shell with a face and a hat, in the crewmate's own look.
/// It breathes, blinks, glances around, scans while working, hops when it needs you and sleeps
/// when its computer is offline. Same viewBox and coordinates as the desktop SVG.
/// This file is the animated widget; the drawing is [CrewmatePainter].
library;

import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_painter.dart';

class Crewmate extends StatefulWidget {
  const Crewmate({
    super.key,
    required this.seed,
    required this.look,
    required this.state,
    this.size = 48,
    this.animate = true,
  });

  /// Crewmate id: decides the colour when the crewmate never picked one, and offsets its motion.
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
  final _motion = ValueNotifier(CrewPose.rest);

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
    _motion.value = CrewPose(
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
        painter: CrewmatePainter(
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
