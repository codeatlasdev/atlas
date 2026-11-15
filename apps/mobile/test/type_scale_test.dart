// One type scale (core/ui/atlas_theme.dart: TextSize + the textTheme roles). A screen takes a
// size from the theme; it does not invent one. This reads every source file under lib/ and
// fails when a line with `fontSize` appears outside the theme, bar the exceptions below. The numbers
// are a ceiling AND a floor: a file that grows fails, and a file that shrinks must lower its
// entry here, so the list only ever gets shorter.
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// Files allowed to name a `fontSize`, with how many times, and why. A count of 0 is not listed.
const _allowed = <String, int>{
  'lib/core/lab/lab.dart': 1,
  'lib/core/ui/attention_card.dart': 2,
  'lib/core/ui/components.dart': 2,
  'lib/core/ui/computer_state.dart': 1,
  'lib/core/ui/floating_tab_bar.dart': 1,
  'lib/core/ui/markdown/atlas_markdown.dart': 11,
  'lib/core/ui/markdown/bar_chart.dart': 1,
  'lib/core/ui/markdown/code_block.dart': 4,
  'lib/core/ui/protocol_banner.dart': 1,
  'lib/features/agents/crew_widgets.dart': 1,
  'lib/features/crew/avatar/crewmate_painter.dart': 1,
  'lib/features/git/changes_screen.dart': 1,
  'lib/features/projects/project_avatar.dart': 1,
  'lib/features/terminal/live_screen.dart': 2,
  'lib/features/terminal/terminal_feed.dart': 1,
};

/// Generated code is not ours to restyle.
const _skipped = ['lib/src/rust/'];

String _stripComments(String src) => src
    .split('\n')
    .map((l) {
      final i = l.indexOf('//');
      return i < 0 ? l : l.substring(0, i);
    })
    .join('\n');

Map<String, int> _fontSizeCounts() {
  final counts = <String, int>{};
  final dir = Directory('lib');
  for (final f in dir.listSync(recursive: true).whereType<File>()) {
    final path = f.path.replaceAll('\\', '/');
    if (!path.endsWith('.dart')) continue;
    if (path == 'lib/core/ui/atlas_theme.dart') continue;
    if (_skipped.any(path.startsWith)) continue;
    final n = _stripComments(f.readAsStringSync())
        .split('\n')
        .where((l) => l.contains('fontSize'))
        .length;
    if (n > 0) counts[path] = n;
  }
  return counts;
}

void main() {
  test('no new fontSize outside the theme, and the exceptions only shrink', () {
    final found = _fontSizeCounts();
    final problems = <String>[];
    for (final e in found.entries) {
      final allowed = _allowed[e.key] ?? 0;
      if (e.value > allowed) {
        problems.add(
          '${e.key}: ${e.value} fontSize (allowed $allowed). Use a textTheme role from '
          'atlas_theme.dart (.copyWith for colour/weight), not a new size.',
        );
      } else if (e.value < allowed) {
        problems.add(
          '${e.key}: only ${e.value} fontSize left, lower its entry from $allowed in this test.',
        );
      }
    }
    for (final k in _allowed.keys) {
      if (!found.containsKey(k)) {
        problems.add('$k: no fontSize left, delete its entry from this test.');
      }
    }
    expect(problems, isEmpty, reason: problems.join('\n'));
  });

  test('TextStyle with a literal size is not written in features/', () {
    // Stricter than the test above, and what the sweep ends on: features/ has no
    // TextStyle(fontSize: <number or expression>). Roles come from the theme.
    final offenders = <String>[];
    for (final f in Directory(
      'lib/features',
    ).listSync(recursive: true).whereType<File>()) {
      if (!f.path.endsWith('.dart')) continue;
      final src = _stripComments(f.readAsStringSync());
      for (final m in RegExp(
        r'TextStyle\(([^()]|\([^()]*\))*fontSize:',
      ).allMatches(src)) {
        final line = '\n'.allMatches(src.substring(0, m.start)).length + 1;
        offenders.add('${f.path.replaceAll('\\', '/')}:$line');
      }
    }
    final allowed = _allowedTextStyleSites;
    final extra = offenders
        .where((o) => !allowed.contains(o.split(':').first))
        .toList();
    expect(extra, isEmpty, reason: extra.join('\n'));
  });
}

/// Files in features/ that still build a TextStyle with a size (temporary: shrinks per commit).
final _allowedTextStyleSites = _allowed.keys
    .where((k) => k.startsWith('lib/features/'))
    .toSet();
