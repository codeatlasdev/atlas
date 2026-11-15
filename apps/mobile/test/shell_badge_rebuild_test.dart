// Regression: the app shell watched the inbox providers to compute the Inbox tab's badge, so every
// crew push (a crewmate state change updates `inboxProvider`) rebuilt the whole shell and with it the
// tab bar and the branch stack. Only the badge depends on the count; the rest must stay put.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/main.dart';

const _mac = MacDevice(nodeId: 'n1', name: 'mac');

AgentAttention _waiting(String pane) => AgentAttention(
  _mac,
  ProjectInfo(
    id: 'p',
    name: 'p',
    root: '/p',
    running: true,
    services: const [],
    agents: const [],
  ),
  pane,
  AgentStatus(state: 'waiting', agent: 'omp', updatedAt: 0, stateStartedAt: 0),
);

/// The "crew push": the list the inbox provider yields changes under the shell.
final _attention = StateProvider<List<Attention>>((_) => const []);

Future<void> _pump(WidgetTester t) async {
  final router = GoRouter(
    routes: [
      StatefulShellRoute.indexedStack(
        builder: (context, state, shell) => ShellScaffold(shell: shell),
        branches: [
          for (final path in ['/', '/crew', '/inbox', '/settings'])
            StatefulShellBranch(
              routes: [
                GoRoute(path: path, builder: (_, _) => const SizedBox.expand()),
              ],
            ),
        ],
      ),
    ],
  );
  addTearDown(router.dispose);
  await t.binding.setSurfaceSize(const Size(393, 852));
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        inboxProvider.overrideWith((ref) => ref.watch(_attention)),
        recentActivityProvider.overrideWith((ref) => const []),
        inboxSeenProvider.overrideWith(_NeverSeen.new),
      ],
      child: MaterialApp.router(theme: atlasTheme(), routerConfig: router),
    ),
  );
  await t.pump();
}

class _NeverSeen extends InboxSeen {
  @override
  DateTime? build() => null;
}

void main() {
  testWidgets(
    'a crew push updates the Inbox badge without rebuilding the shell',
    (t) async {
      await _pump(t);
      final container = ProviderScope.containerOf(
        t.element(find.byType(ShellScaffold)),
      );
      int badge() =>
          t.widget<FloatingTabBar>(find.byType(FloatingTabBar)).inboxCount;
      expect(badge(), 0);

      // Which widgets the framework rebuilds while the push is applied.
      final rebuilt = <Type>[];
      final prior = debugOnRebuildDirtyWidget;
      debugOnRebuildDirtyWidget = (element, builtOnce) =>
          rebuilt.add(element.widget.runtimeType);
      addTearDown(() => debugOnRebuildDirtyWidget = prior);

      container.read(_attention.notifier).state = [
        _waiting('a'),
        _waiting('b'),
      ];
      await t.pump();

      expect(badge(), 2, reason: 'the badge still follows the inbox');
      expect(
        rebuilt,
        isNot(contains(ShellScaffold)),
        reason: 'the whole shell rebuilt for a badge count',
      );
    },
  );
}
