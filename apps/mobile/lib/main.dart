import 'package:flutter/material.dart';

import 'package:atlas_mobile/src/rust/frb_generated.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/fade_branches.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/pair/pair_screen.dart';
import 'package:atlas_mobile/features/projects/project_detail_screen.dart';
import 'package:atlas_mobile/features/settings/settings_screen.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';

final _rootNavigatorKey = GlobalKey<NavigatorState>();

final _router = GoRouter(
  navigatorKey: _rootNavigatorKey,
  routes: [
    // ── Shell: one branch per tab; the bar is visible here ─────────────────
    StatefulShellRoute(
      builder: (context, state, shell) => _ShellScaffold(shell: shell),
      navigatorContainerBuilder: (context, shell, children) =>
          FadeBranches(index: shell.currentIndex, children: children),
      branches: [
        // Branch 0 – Computers
        StatefulShellBranch(
          routes: [GoRoute(path: '/', builder: (_, _) => const HomeScreen())],
        ),
        // Branch 1 – Atlas Crew
        StatefulShellBranch(
          routes: [
            GoRoute(path: '/crew', builder: (_, _) => const CrewScreen()),
          ],
        ),
        // Branch 2 – Inbox
        StatefulShellBranch(
          routes: [
            GoRoute(path: '/inbox', builder: (_, _) => const InboxScreen()),
          ],
        ),
        // Branch 3 – Settings
        StatefulShellBranch(
          routes: [
            GoRoute(
              path: '/settings',
              builder: (_, _) => const SettingsScreen(),
            ),
          ],
        ),
      ],
    ),

    // ── Detail routes on the root navigator: bar hides while these are open
    GoRoute(
      parentNavigatorKey: _rootNavigatorKey,
      path: '/pair',
      builder: (_, _) => const PairScreen(),
    ),
    GoRoute(
      parentNavigatorKey: _rootNavigatorKey,
      path: '/project',
      builder: (_, state) {
        final args = state.extra! as Map<String, dynamic>;
        return ProjectDetailScreen(
          device: args['device'] as MacDevice,
          project: args['project'] as ProjectInfo,
        );
      },
    ),
    GoRoute(
      parentNavigatorKey: _rootNavigatorKey,
      path: '/terminal',
      builder: (_, state) {
        final args = state.extra! as Map<String, dynamic>;
        return TerminalScreen(
          device: args['device'] as MacDevice,
          pty: args['pty'] as String,
          title: args['title'] as String?,
          pane: args['pane'] as String?,
        );
      },
    ),
    GoRoute(
      parentNavigatorKey: _rootNavigatorKey,
      path: '/crew/:nodeId/:crewmateId',
      builder: (_, state) {
        final args = (state.extra ?? {}) as Map<String, dynamic>;
        return CrewmateChatScreen(
          device: args['device'] as MacDevice,
          crewmateId: state.pathParameters['crewmateId']!,
          scrollToTaskId: args['scrollToTaskId'] as String?,
        );
      },
    ),
  ],
);

// ── Shell scaffold ─────────────────────────────────────────────────────────

class _ShellScaffold extends ConsumerWidget {
  const _ShellScaffold({required this.shell});
  final StatefulNavigationShell shell;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Scaffold(
      body: Stack(
        children: [
          shell,
          Positioned(
            bottom: 0,
            left: Space.lg,
            right: Space.lg,
            child: SafeArea(
              top: false,
              child: Padding(
                padding: const EdgeInsets.only(bottom: Space.md),
                child: FloatingTabBar(
                  currentIndex: shell.currentIndex,
                  onTabSelected: (i) => shell.goBranch(
                    i,
                    initialLocation: i == shell.currentIndex,
                  ),
                  inboxCount:
                      ref.watch(inboxProvider).length +
                      unreadCount(
                        ref.watch(recentActivityProvider),
                        ref.watch(inboxSeenProvider),
                      ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

// ── App bootstrap ──────────────────────────────────────────────────────────

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  runApp(const ProviderScope(child: AtlasMobileApp()));
}

class AtlasMobileApp extends ConsumerStatefulWidget {
  const AtlasMobileApp({super.key});

  @override
  ConsumerState<AtlasMobileApp> createState() => _AtlasMobileAppState();
}

class _AtlasMobileAppState extends ConsumerState<AtlasMobileApp> {
  // Tells each live connection when the app leaves and returns, so a link that
  // died in the background is probed and replaced on return instead of making
  // the first screen wait out its request deadline.
  late final AppLifecycleListener _life = AppLifecycleListener(
    onResume: () => _each((n) => n.onForegrounded()),
    onHide: () => _each((n) => n.onBackgrounded()),
  );

  void _each(void Function(ConnectionNotifier) f) {
    final devices = ref.read(pairedDevicesProvider).value ?? const [];
    for (final d in devices) {
      final p = connectionNotifierProvider(d);
      if (ref.exists(p)) f(ref.read(p.notifier));
    }
  }

  @override
  void initState() {
    super.initState();
    _life;
  }

  @override
  void dispose() {
    _life.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp.router(
      title: 'Atlas',
      debugShowCheckedModeBanner: false,
      theme: atlasTheme(),
      routerConfig: _router,
    );
  }
}
