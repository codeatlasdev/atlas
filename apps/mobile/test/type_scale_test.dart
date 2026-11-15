// One type scale (core/ui/atlas_theme.dart: TextSize + the textTheme roles). A screen takes a
// size from the theme; it does not invent one. This reads every source file under lib/ and
// fails when a `fontSize` is a number or arithmetic on one, bar the exceptions below. A role is
// read as `Theme.of(context).textTheme.bodyMedium` and varied with `.copyWith(color:, fontWeight:)`.
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// Files allowed to compute a `fontSize` from geometry, how many lines, and why. The number is a
/// ceiling AND a floor: a file that grows fails, and one that shrinks must lower its entry.
const _computed = <String, (int, String)>{
  'lib/features/projects/project_avatar.dart': (
    1,
    'the initial scales with the avatar circle',
  ),
  'lib/features/agents/crew_widgets.dart': (
    1,
    'the initial scales with the avatar circle',
  ),
  'lib/features/crew/avatar/crewmate_painter.dart': (
    1,
    'the sleeping "z" is drawn on the canvas and grows with the animation',
  ),
  'lib/features/terminal/terminal_feed.dart': (
    1,
    'the feed is a monospace grid; its size is set by the Mac pty columns',
  ),
  'lib/features/terminal/live_screen.dart': (
    2,
    'the live terminal fits the Mac pty columns; the 10 pt probe measures the mono cell',
  ),
};

/// Generated code is not ours to restyle.
const _skipped = ['lib/src/rust/', 'lib/core/ui/atlas_theme.dart'];

String _stripComments(String src) => src
    .split('\n')
    .map((l) {
      final i = l.indexOf('//');
      return i < 0 ? l : l.substring(0, i);
    })
    .join('\n');

Iterable<File> _sources() =>
    Directory('lib')
        .listSync(recursive: true)
        .whereType<File>()
        .where((f) => f.path.endsWith('.dart'))
        .where((f) {
          final p = f.path.replaceAll('\\', '/');
          return !_skipped.any(p.startsWith);
        });

String _rel(File f) => f.path.replaceAll('\\', '/');

void main() {
  test('no fontSize is a number or arithmetic on one outside the theme', () {
    final bad = <String>[];
    // `fontSize: 12`, `fontSize: s * .38`, `fontSize: (x ?? 14.5) - 1`, `this.fontSize = 12.5`.
    final numeric = RegExp(r'fontSize\s*[:=]\s*[^,;)]*?[0-9]');
    for (final f in _sources()) {
      final lines = _stripComments(f.readAsStringSync()).split('\n');
      for (var i = 0; i < lines.length; i++) {
        final l = lines[i];
        if (!l.contains('fontSize')) continue;
        if (!numeric.hasMatch(l)) continue;
        if (_computed.containsKey(_rel(f))) continue;
        bad.add(
          '${_rel(f)}:${i + 1}: `${l.trim()}`. Use a textTheme role from atlas_theme.dart '
          '(.copyWith for colour or weight) or TextSize.*, not a new size.',
        );
      }
    }
    expect(bad, isEmpty, reason: bad.join('\n'));
  });

  test('the computed-size exceptions only shrink', () {
    final problems = <String>[];
    final found = <String, int>{};
    for (final f in _sources()) {
      final path = _rel(f);
      if (!_computed.containsKey(path)) continue;
      found[path] = _stripComments(f.readAsStringSync())
          .split('\n')
          .where((l) => l.contains('fontSize'))
          .length;
    }
    for (final e in _computed.entries) {
      final n = found[e.key] ?? 0;
      if (n > e.value.$1) {
        problems.add(
          '${e.key}: $n fontSize lines (allowed ${e.value.$1}: ${e.value.$2}).',
        );
      } else if (n < e.value.$1) {
        problems.add(
          '${e.key}: only $n fontSize lines left, lower or delete its entry (was ${e.value.$1}).',
        );
      }
    }
    expect(problems, isEmpty, reason: problems.join('\n'));
  });

  test('features/ builds no TextStyle with a size of its own', () {
    // The sweep ends here: a screen reads a role, so `TextStyle(` never carries a fontSize in
    // features/, bar the geometry-computed files above.
    final offenders = <String>[];
    final style = RegExp(r'TextStyle\(([^()]|\([^()]*\))*fontSize:');
    for (final f in _sources().where(
      (f) => _rel(f).startsWith('lib/features/'),
    )) {
      if (_computed.containsKey(_rel(f))) continue;
      final src = _stripComments(f.readAsStringSync());
      for (final m in style.allMatches(src)) {
        final line = '\n'.allMatches(src.substring(0, m.start)).length + 1;
        offenders.add('${_rel(f)}:$line');
      }
    }
    expect(offenders, isEmpty, reason: offenders.join('\n'));
  });
}
