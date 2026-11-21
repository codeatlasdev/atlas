import 'dart:convert';
import 'dart:io';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';

import 'support/demo_harness.dart';

// Regression (screenshots 2026-11-20): the home listed every registry entry flat (dozens of worktrees of one
// repository, `Downloads` and `.omp` as if they were projects) and had no way to find a project in a long list.
// On hostd's real `project_list` (atlas-host's projects::tests writes the fixture) plus more projects: one row per
// repository, idle worktrees folded behind it, folders in a closed section, and a search over all of it.

const _device = MacDevice(nodeId: 'n1', name: 'Mac');

List<ProjectInfo> _hostdList() {
  final data = jsonDecode(
    File('test/fixtures/project_list.json').readAsStringSync(),
  ) as Map<String, dynamic>;
  return [
    for (final p in data['projects'] as List)
      ProjectInfo.fromJson(p as Map<String, dynamic>),
    // Enough projects for the list to need a search.
    for (final n in ['ac', 'wi', 'zelvo', 'digital', 'r2k'])
      ProjectInfo(
        id: n,
        root: '/Users/me/$n',
        name: n,
        running: false,
        services: const [],
        repo: '/Users/me/$n',
        branch: 'main',
      ),
  ];
}

Future<void> _pump(WidgetTester t, List<ProjectInfo> list) async {
  await t.binding.setSurfaceSize(const Size(393, 1400));
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(DemoStorage([_device])),
        transportFactoryProvider.overrideWithValue(
          (_, _) async => demoTransport(),
        ),
        projectListProvider(_device).overrideWith((ref) async => list),
      ],
      child: MaterialApp(theme: atlasTheme(), home: const HomeScreen()),
    ),
  );
  await settle(t);
}

void main() {
  testWidgets(
    'a worktree folds under its project, a folder under Other folders',
    (t) async {
      await _pump(t, _hostdList());

      expect(find.text('shop'), findsOneWidget);
      // The worktree is idle: not a row of its own until its project is unfolded.
      expect(find.text('feature/x'), findsNothing);
      expect(find.text('Downloads'), findsNothing);

      await t.tap(find.text('1 worktree'));
      await settle(t);
      expect(find.text('feature/x'), findsOneWidget);
      await t.tap(find.text('Hide worktrees'));
      await settle(t);
      expect(find.text('feature/x'), findsNothing);

      await t.tap(find.byKey(const Key('other-folders')));
      await settle(t);
      expect(find.text('Downloads'), findsOneWidget);
    },
  );

  testWidgets('an active worktree shows under its project without unfolding', (
    t,
  ) async {
    final list = [
      for (final p in _hostdList())
        p.id == 'w'
            ? ProjectInfo(
                id: p.id,
                root: p.root,
                name: p.name,
                running: true,
                services: const [],
                repo: p.repo,
                branch: p.branch,
                worktree: true,
              )
            : p,
    ];
    await _pump(t, list);
    expect(find.text('feature/x'), findsOneWidget);
    expect(find.text('1 worktree'), findsNothing, reason: 'none left folded');
  });

  testWidgets('search finds a worktree by branch and a folder by name', (
    t,
  ) async {
    await _pump(t, _hostdList());
    final search = find.byKey(const Key('project-search'));
    expect(search, findsOneWidget);

    await t.enterText(search, 'feature');
    await settle(t);
    expect(find.text('feature/x'), findsOneWidget);
    expect(find.text('ac'), findsNothing);
    expect(find.text('shop'), findsNothing);

    await t.enterText(search, 'downl');
    await settle(t);
    expect(find.text('Downloads'), findsOneWidget);

    await t.enterText(search, 'nothing-like-this');
    await settle(t);
    expect(find.text('Nothing matches "nothing-like-this"'), findsOneWidget);

    await t.tap(find.byTooltip('Clear search'));
    await settle(t);
    expect(find.text('ac'), findsOneWidget);
    expect(find.text('shop'), findsOneWidget);
  });

  testWidgets('a short list has no search field', (t) async {
    await _pump(t, _hostdList().take(3).toList());
    expect(find.byKey(const Key('project-search')), findsNothing);
  });
}
