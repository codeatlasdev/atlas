/// What a crewmate looks like and how it feels: the `look` (colour, eyes, hat) from `crewmate_list`,
/// the colour helpers ported from the desktop's `look.ts`, and the presence state the face shows.
library;

import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

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

/// What a crewmate looks like. Built from the untrusted `look` of `crewmate_list`.
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

/// A crewmate's presence, as the face shows it.
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

  /// How the state reads in a label, lowercase ("on deck" is idle).
  String get word => switch (this) {
    CrewState.working => 'working',
    CrewState.needsYou => 'needs you',
    CrewState.offline => 'offline',
    CrewState.idle => 'on deck',
  };

  /// The state's colour in the UI.
  Color color(AtlasColors c) => switch (this) {
    CrewState.working => c.accent,
    CrewState.needsYou => c.warning,
    CrewState.offline => c.faint,
    CrewState.idle => c.success,
  };

  /// Working and needs-you pulse; the others sit still.
  bool get pulses => this == CrewState.working || this == CrewState.needsYou;
}
