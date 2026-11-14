import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agent_launcher.dart';
import 'package:atlas_mobile/features/projects/worktree.dart';
import 'package:atlas_mobile/features/projects/worktrees_section.dart';

import 'support/demo_harness.dart';

Map<String, dynamic> _fixture(String name) =>
    jsonDecode(File('test/fixtures/$name').readAsStringSync())
        as Map<String, dynamic>;

const _mac = MacDevice(nodeId: 'demo', name: 'Demo Mac', addrs: []);

const _agents = [
  {'id': 'claude', 'label': 'Claude Code', 'state': 'ok'},
];

/// The demo computer, recording every request the app sends it.
class _Spy implements Transport {
  _Spy(this.inner, this.calls);
  final Transport inner;
  final List<Map<String, dynamic>> calls;
  @override
  Stream<String> get incomingLines => inner.incomingLines;
  @override
  Future<void> sendLine(String line) {
    calls.add(jsonDecode(line) as Map<String, dynamic>);
    return inner.sendLine(line);
  }

  @override
  Future<void> close() => inner.close();
}

List<Override> _overrides(Transport t) => [
  macStorageProvider.overrideWithValue(DemoStorage(const [_mac])),
  transportFactoryProvider.overrideWithValue((_, _) async => t),
];

void main() {
  // Real hostd output (atlas-host worktrees::local::tests::the_phone_fixtures_are_the_real_payloads).
  test(
    'worktree_list from hostd parses: main first, dirty and branch kept',
    () {
      final l = WorktreeList.fromJson(_fixture('worktree_list.json'));
      expect(l.repoId, 'repo0000000a');
      expect(l.worktrees.map((w) => w.name), ['shop', 'phone-task']);
      final main = l.worktrees.first;
      expect((main.main, main.branch, main.dirty), (true, 'main', false));
      final wt = l.worktrees.last;
      expect(
        (wt.main, wt.branch, wt.dirty, wt.portOffset),
        (false, 'phone-task', true, 100),
      );
      expect(wt.id, 'wt00000000b1');
    },
  );

  test(
    'worktree_create from hostd parses, with and without a setup result',
    () {
      final made = CreatedWorktree.fromJson(_fixture('worktree_create.json'));
      expect(made.worktree.id, 'wt00000000b1');
      expect(made.worktree.branch, 'phone-task');
      expect(made.setup, isNull, reason: 'no worktree.setup configured');
      final failed = CreatedWorktree.fromJson({
        ..._fixture('worktree_create.json'),
        'setup': {'ok': false, 'code': 3, 'output': 'boom\n'},
      });
      expect((failed.setup!.ok, failed.setup!.output), (false, 'boom\n'));
    },
  );

  test('a removal answer is either removed or kept with the reason', () {
    expect(
      WorktreeRemoval.fromJson({'removed': true, 'branch': 'x'}).removed,
      isTrue,
    );
    final kept = WorktreeRemoval.fromJson({'kept': 'unpushed'});
    expect((kept.removed, kept.kept), (false, 'unpushed'));
    expect(keptReason('dirty'), contains('uncommitted'));
    expect(keptReason('secrets'), contains('.env'));
    expect(keptReason('unpushed'), contains('pushed'));
  });

  testWidgets(
    'the section lists worktrees, creates one and removes a clean one',
    (t) async {
      await t.pumpWidget(
        demoWrap(
          const Scaffold(
            body: SingleChildScrollView(
              child: WorktreesSection(device: _mac, projectId: 'demo-shop'),
            ),
          ),
          const [_mac],
        ),
      );
      await settle(t);
      expect(find.text('Worktrees'), findsOneWidget);
      expect(find.text('main'), findsWidgets);
      expect(find.text('fix-checkout'), findsWidgets);

      await t.tap(find.text('New worktree'));
      await settle(t);
      await t.enterText(find.byType(TextField), 'Phone Task');
      await t.tap(find.text('Create'));
      await settle(t);
      expect(find.text('phone-task'), findsWidgets);

      // The dirty demo worktree is kept and says why.
      await t.tap(find.byTooltip('Remove fix-checkout'));
      await settle(t);
      await t.tap(find.text('Remove'));
      await settle(t);
      expect(find.textContaining('uncommitted'), findsOneWidget);
      expect(find.text('fix-checkout'), findsWidgets);

      // A clean one goes.
      await t.tap(find.byTooltip('Remove phone-task'));
      await settle(t);
      await t.tap(find.text('Remove'));
      await settle(t);
      expect(find.text('phone-task'), findsNothing);
    },
  );

  testWidgets(
    'the launcher offers the worktrees and starts the agent in the chosen one',
    (t) async {
      final calls = <Map<String, dynamic>>[];
      final transport = demoTransport(integrations: _agents);
      LaunchedAgent? result;
      await t.pumpWidget(
        ProviderScope(
          overrides: [..._overrides(_Spy(transport, calls))],
          child: MaterialApp(
            theme: atlasTheme(),
            home: Builder(
              builder: (context) => Scaffold(
                body: TextButton(
                  onPressed: () async => result = await showAgentLauncher(
                    context,
                    device: _mac,
                    projectId: 'demo-shop',
                  ),
                  child: const Text('open'),
                ),
              ),
            ),
          ),
        ),
      );
      await t.tap(find.text('open'));
      await settle(t);
      expect(find.text('Where'), findsOneWidget);
      await t.tap(find.text('fix-checkout'));
      await t.pump();
      await t.tap(find.text('Start agent'));
      await settle(t);
      final launch = calls.lastWhere((c) => c['cmd'] == 'agent_launch');
      expect(launch['args']['worktree'], 'demo-wt-fix');
      expect(result, isNotNull);
    },
  );

  testWidgets(
    'the launcher starts in the main checkout when no worktree is picked',
    (t) async {
      final calls = <Map<String, dynamic>>[];
      await t.pumpWidget(
        ProviderScope(
          overrides: [
            ..._overrides(_Spy(demoTransport(integrations: _agents), calls)),
          ],
          child: MaterialApp(
            theme: atlasTheme(),
            home: Builder(
              builder: (context) => Scaffold(
                body: TextButton(
                  onPressed: () => showAgentLauncher(
                    context,
                    device: _mac,
                    projectId: 'demo-shop',
                  ),
                  child: const Text('open'),
                ),
              ),
            ),
          ),
        ),
      );
      await t.tap(find.text('open'));
      await settle(t);
      await t.tap(find.text('Start agent'));
      await settle(t);
      final launch = calls.lastWhere((c) => c['cmd'] == 'agent_launch');
      expect((launch['args'] as Map).containsKey('worktree'), isFalse);
    },
  );
}
