/// Tests for steps.dart — mirrors apps/desktop/src/lib/bots/stepPhrase.test.ts cases.
library;

import 'package:flutter_test/flutter_test.dart';
import 'package:atlas_mobile/features/bots/steps.dart';

// ── test step type ────────────────────────────────────────────────────────────

class _Step {
  const _Step(this.text, {this.group, this.ts = 0});
  final String text;
  final String? group;
  final int ts;
}

// Helper: call stepGroups with the _Step extractor.
List<StepGroup<_Step>> _groups(
  List<_Step> steps,
  bool running, {
  String? fallback,
}) => stepGroups(steps, running, groupOf: (s) => s.group, fallback: fallback);

void main() {
  // ── stepPhrase ──────────────────────────────────────────────────────────────

  group('stepPhrase', () {
    test('reading a file names the file, not its full path', () {
      expect(
        stepPhrase(
          'Read /var/lib/atlas-bot/.atlas/bots/x/work/src/routes/login.svelte',
        ),
        'Reading login.svelte',
      );
      expect(stepPhrase('read src/app.ts'), 'Reading app.ts');
    });

    test('edits and writes', () {
      expect(stepPhrase('Edit src/lib/api.ts'), 'Editing api.ts');
      expect(stepPhrase('write CHANGELOG.md'), 'Writing CHANGELOG.md');
    });

    test('shell commands say what kind of command, by its program', () {
      expect(stepPhrase('Bash git status'), 'Checking git');
      expect(stepPhrase('bash bun test src/lib'), 'Running the tests');
      expect(stepPhrase('bash cargo test -p atlas-host'), 'Running the tests');
      expect(stepPhrase('bash npm install'), 'Installing dependencies');
      expect(stepPhrase('bash gh pr create --fill'), 'Working on GitHub');
      expect(stepPhrase('bash ls -la'), 'Running ls');
    });

    test('searching', () {
      expect(stepPhrase('Grep handleLogin'), 'Searching for "handleLogin"');
      expect(stepPhrase('glob **/*.svelte'), 'Looking for files');
    });

    test(
      'a tool it does not know still reads as a sentence, never a raw id',
      () {
        expect(
          stepPhrase('mcp__linear__save_issue {"x":1}'),
          'Using save issue',
        );
        expect(stepPhrase('todo'), 'Using todo');
      },
    );

    test('nothing yet is the plain working line', () {
      expect(stepPhrase(null), 'Working');
      expect(stepPhrase(''), 'Working');
    });
  });

  // ── workingLabel ────────────────────────────────────────────────────────────

  group('workingLabel', () {
    int at(int s) => s * 1000;

    test('a fresh step shows what it did', () {
      expect(workingLabel('Read src/app.ts', at(10), at(11)), 'Reading app.ts');
    });

    test('after a few quiet seconds it cycles idle words, changing every few seconds', () {
      final a = workingLabel('Read src/app.ts', at(10), at(16));
      final b = workingLabel('Read src/app.ts', at(10), at(20));
      expect(idleWords, contains(a));
      expect(idleWords, contains(b));
      expect(a, isNot(equals(b)));
    });

    test('with no step yet it starts cycling right away', () {
      expect(idleWords, contains(workingLabel(null, null, at(1))));
    });

    test(
      'the same moment always gives the same word (no flicker on re-render)',
      () {
        expect(
          workingLabel(null, null, at(7)),
          workingLabel(null, null, at(7)),
        );
      },
    );
  });

  // ── stepGroups ──────────────────────────────────────────────────────────────

  group('stepGroups', () {
    _Step s(String text, [String? grp, int ts = 0]) =>
        _Step(text, group: grp, ts: ts);

    test('consecutive steps under one title form one group; a new title opens the next', () {
      final g = _groups([
        s('read a.ts', 'CART: Testar'),
        s('bash bun test', 'CART: Testar'),
        s('edit b.ts', 'FIX: Corrigir'),
      ], true);
      expect(g.map((x) => [x.title, x.steps.length, x.done]).toList(), [
        ['CART: Testar', 2, true],
        ['FIX: Corrigir', 1, false],
      ]);
    });

    test('steps before omp\'s first real title join that first group instead of an untitled one', () {
      final g = _groups([
        s('read memory.md'),
        s('bash git status'),
        s('read a.ts', 'CART: Testar'),
      ], true);
      expect(g.map((x) => [x.title, x.steps.length]).toList(), [
        ['CART: Testar', 3],
      ]);
    });

    test('when the task is over every group is done; with no title at all there is one untitled group', () {
      expect(
        _groups([
          s('read a.ts', 'A'),
          s('read b.ts', 'B'),
        ], false).every((x) => x.done),
        true,
      );
      expect(
        _groups([s('read a.ts')], true).map((x) => [x.title, x.done]).toList(),
        [
          [null, false],
        ],
      );
      expect(_groups(<_Step>[], true), isEmpty);
    });

    // Seen live: omp's title reached the task only after its 12 steps, so none carried a group.
    test('steps that never got a title take the task\'s own title', () {
      final g = _groups(
        [s('read a.ts'), s('read b.ts')],
        false,
        fallback: 'STORE: Explicar',
      );
      expect(g.map((x) => [x.title, x.steps.length]).toList(), [
        ['STORE: Explicar', 2],
      ]);
      expect(
        _groups(
          [s('read a', 'A')],
          false,
          fallback: 'T',
        ).map((x) => x.title).toList(),
        ['A'],
      );
    });

    test('a title that comes back later is a new group, so the order of the work is kept', () {
      final g = _groups([
        s('read a', 'A'),
        s('read b', 'B'),
        s('read c', 'A'),
      ], false);
      expect(g.map((x) => x.title).toList(), ['A', 'B', 'A']);
    });
  });

  // ── hashline and leading cd ─────────────────────────────────────────────────

  // Incident: edit target was [path#TAG] but phrase showed the raw anchor.
  test(
    'an omp hashline edit target names the file, not the [path#TAG] anchor',
    () {
      expect(
        stepPhrase(
          'edit [apps/store/src/lib/analytics/footer-contact.test.ts#07A1]',
        ),
        'Editing footer-contact.test.ts',
      );
      // hostd keeps 80 characters of a step, which can cut the closing bracket.
      expect(
        stepPhrase(
          'edit [apps/store/src/lib/analytics/footer-contact.test.ts#07',
        ),
        'Editing footer-contact.test.ts',
      );
    },
  );

  // Seen live: the agent runs "cd apps/store && bun test" and the card said "Running cd".
  test('a leading cd into a folder does not hide the command it runs', () {
    expect(
      stepPhrase('bash cd apps/store && bun test 2>&1 | tail'),
      'Running the tests',
    );
    expect(stepPhrase('bash cd apps/store && git status'), 'Checking git');
    expect(stepPhrase('bash cd /tmp; ls -la'), 'Running ls');
    expect(stepPhrase('bash cd apps/store'), 'Running cd');
  });
}
