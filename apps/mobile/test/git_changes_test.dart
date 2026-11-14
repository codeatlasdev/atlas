import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/markdown/code_block.dart';
import 'package:atlas_mobile/features/git/git_models.dart';
import 'package:atlas_mobile/features/git/changes_screen.dart';

import 'support/demo_harness.dart';

Map<String, dynamic> _fixture(String name) =>
    jsonDecode(File('test/fixtures/$name').readAsStringSync())
        as Map<String, dynamic>;

const _mac = MacDevice(nodeId: 'demo', name: 'Demo Mac');

const _project = ProjectInfo(
  id: 'demo-shop',
  root: '/Users/demo/Projetos/orbit-shop',
  name: 'orbit-shop',
  running: true,
  services: [],
);

void main() {
  // Real hostd output (atlas-host git::tests::payloads_match_the_phone_fixtures): a mock here could
  // hide a renamed key.
  group('payloads from hostd', () {
    test('status: staged and unstaged halves of one file, untracked apart', () {
      final s = GitStatus.fromJson(_fixture('git_status.json'));
      expect(s.branch, 'main');
      expect(s.upstream, isNull);
      final a = s.entries.firstWhere((e) => e.path == 'a.txt');
      expect((a.hasStaged, a.hasUnstaged), (true, true));
      final n = s.entries.firstWhere((e) => e.path == 'new.txt');
      expect((n.hasStaged, n.hasUnstaged, n.untracked), (false, true, true));
      expect(s.staged.map((e) => e.path), ['a.txt']);
      expect(s.unstaged.map((e) => e.path), ['a.txt', 'new.txt']);
    });

    test('diff: line kinds, numbers and the untracked file as additions', () {
      final d = GitDiff.fromJson(_fixture('git_diff.json'));
      expect(d.truncated, isFalse);
      final a = d.files.firstWhere((f) => f.path == 'a.txt');
      expect((a.status, a.staged), ('modified', false));
      expect(a.hunks.single.lines.map((l) => l.kind), [
        'ctx',
        'ctx',
        'ctx',
        'add',
      ]);
      expect(a.hunks.single.lines.last.newNo, 4);
      expect(a.hunks.single.lines.last.oldNo, isNull);
      final n = d.files.firstWhere((f) => f.path == 'new.txt');
      expect(n.status, 'untracked');
      expect(n.hunks.single.lines.single.text, 'fresh');
    });

    test('a diff renders as the unified text the code block tints', () {
      final d = GitDiff.fromJson(_fixture('git_diff.json'));
      final text = d.files.first.asPatch();
      expect(text.split('\n').first, '@@ -1,3 +1,4 @@');
      expect(text, contains('\n+four'));
      expect(text, contains('\n one'));
    });

    test('commit: the short sha', () {
      expect(_fixture('git_commit.json')['sha'], 'abc1234');
    });
  });

  group('Changes screen against the demo computer', () {
    Future<void> open(WidgetTester t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        demoWrap(const ChangesScreen(device: _mac, project: _project), [_mac]),
      );
      await settle(t);
    }

    testWidgets('lists staged and unstaged files with the branch', (t) async {
      await open(t);
      expect(find.text('feat/discounts'), findsWidgets);
      expect(find.text('Staged'), findsOneWidget);
      expect(find.text('Changes'), findsWidgets);
      // One file is staged, the other two wait for it.
      expect(
        find.byKey(const ValueKey('row-s:src/checkout.ts')),
        findsOneWidget,
      );
      expect(find.byKey(const ValueKey('row-u:README.md')), findsOneWidget);
      expect(
        find.byKey(const ValueKey('row-u:src/discount.ts')),
        findsOneWidget,
      );
    });

    testWidgets('a stage toggle moves the file between the lists', (t) async {
      await open(t);
      final toggle = find.byKey(const ValueKey('stage-README.md'));
      expect(toggle, findsOneWidget);
      await t.tap(toggle);
      await settle(t);
      // Now it is staged: its toggle unstages.
      expect(find.byKey(const ValueKey('unstage-README.md')), findsOneWidget);
      await t.tap(find.byKey(const ValueKey('unstage-README.md')));
      await settle(t);
      expect(find.byKey(const ValueKey('stage-README.md')), findsOneWidget);
    });

    testWidgets('tapping a file shows its diff, tinted by the code block', (
      t,
    ) async {
      await open(t);
      await t.tap(find.text('src/checkout.ts'));
      await settle(t);
      expect(
        find.textContaining('applyDiscount', findRichText: true),
        findsWidgets,
      );
      // The diff is the app's code block in `diff` mode, not a second renderer.
      expect(find.byType(AtlasCodeBlock), findsOneWidget);
    });

    testWidgets('commit sheet: AI message fills the box, commit clears list', (
      t,
    ) async {
      await open(t);
      await t.tap(find.byKey(const ValueKey('commit-button')));
      await settle(t);
      await t.tap(find.byKey(const ValueKey('ai-message')));
      await settle(t);
      final field = t.widget<TextField>(
        find.byKey(const ValueKey('commit-message')),
      );
      expect(field.controller!.text, startsWith('feat(checkout)'));
      await t.tap(find.byKey(const ValueKey('commit-confirm')));
      await settle(t);
      // Nothing staged is left; the unstaged files stay, and the commit is waiting to be pushed.
      expect(find.text('Staged'), findsNothing);
      expect(find.byKey(const ValueKey('row-u:README.md')), findsOneWidget);
      expect(find.byKey(const ValueKey('push-button')), findsOneWidget);
    });

    testWidgets('an empty message cannot be committed', (t) async {
      await open(t);
      await t.tap(find.byKey(const ValueKey('commit-button')));
      await settle(t);
      final confirm = t.widget<FilledButton>(
        find.byKey(const ValueKey('commit-confirm')),
      );
      expect(confirm.onPressed, isNull);
    });

    testWidgets('push then open a pull request', (t) async {
      await open(t);
      await t.tap(find.byKey(const ValueKey('commit-button')));
      await settle(t);
      await t.enterText(
        find.byKey(const ValueKey('commit-message')),
        'fix: demo',
      );
      await t.pump();
      await t.tap(find.byKey(const ValueKey('commit-confirm')));
      await settle(t);
      await t.tap(find.byKey(const ValueKey('push-button')));
      await settle(t);
      expect(find.byKey(const ValueKey('pr-button')), findsOneWidget);
      await t.tap(find.byKey(const ValueKey('pr-button')));
      await settle(t);
      await t.enterText(find.byKey(const ValueKey('pr-title')), 'Demo PR');
      await t.pump();
      await t.tap(find.byKey(const ValueKey('pr-confirm')));
      await settle(t);
      expect(find.textContaining('/pull/'), findsWidgets);
    });
  });
}
