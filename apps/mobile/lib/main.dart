import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/src/rust/frb_generated.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_provider.dart';
import 'package:atlas_mobile/core/push/push_tap.dart';
import 'package:atlas_mobile/core/push/push_service.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/backdrop.dart';
import 'package:atlas_mobile/core/ui/fade_branches.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/core/ui/route_page.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/home/onboarding.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/pair/pair_screen.dart';
import 'package:atlas_mobile/features/projects/project_detail_screen.dart';
import 'package:atlas_mobile/features/settings/settings_screen.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';

final _rootNavigatorKey = GlobalKey<NavigatorState>();

final _router = buildRouter();

/// The app's routes. Tests build their own router to drive the real shell (one at a time: the root navigator's
/// key is shared).
@visibleForTesting
GoRouter buildRouter() => GoRouter(
  navigatorKey: _rootNavigatorKey,
  routes: [
    // ── Shell: one branch per tab; the bar is visible here ─────────────────
    StatefulShellRoute(
      builder: (context, state, shell) => ShellScaffold(shell: shell),
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
      pageBuilder: (context, state) =>
          routePage(context, state, const PairScreen()),
    ),
    GoRoute(
      parentNavigatorKey: _rootNavigatorKey,
      path: '/project',
      pageBuilder: (context, state) {
        final args = state.extra! as Map<String, dynamic>;
        return routePage(
          context,
          state,
          ProjectDetailScreen(
            device: args['device'] as MacDevice,
            project: args['project'] as ProjectInfo,
          ),
        );
      },
    ),
    GoRoute(
      parentNavigatorKey: _rootNavigatorKey,
      path: '/terminal',
      pageBuilder: (context, state) {
        final args = state.extra! as Map<String, dynamic>;
        return routePage(
          context,
          state,
          TerminalScreen(
            device: args['device'] as MacDevice,
            pty: args['pty'] as String,
            title: args['title'] as String?,
            pane: args['pane'] as String?,
          ),
        );
      },
    ),
    GoRoute(
      parentNavigatorKey: _rootNavigatorKey,
      path: '/crew/:nodeId/:crewmateId',
      pageBuilder: (context, state) {
        final args = (state.extra ?? {}) as Map<String, dynamic>;
        return routePage(
          context,
          state,
          CrewmateChatScreen(
            device: args['device'] as MacDevice,
            crewmateId: state.pathParameters['crewmateId']!,
            scrollToTaskId: args['scrollToTaskId'] as String?,
          ),
        );
      },
    ),
  ],
);

// ── Shell scaffold ─────────────────────────────────────────────────────────

/// The tabs' frame: the branch stack and the floating bar, or, on the first run, the onboarding full screen.
/// The tabs stay mounted underneath (offstage, tickers off), so finishing the onboarding reveals a home that
/// has already started connecting, and nothing of the tabs (the bar, a tab's content) ever shows through it.
/// Its only provider is [firstRunProvider], which changes once; the Inbox count lives in
/// [_InboxBadgedTabBar] so a crew push does not rebuild the shell.
class ShellScaffold extends ConsumerWidget {
  const ShellScaffold({super.key, required this.shell});
  final StatefulNavigationShell shell;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final firstRun = ref.watch(firstRunProvider) ?? false;
    return Scaffold(
      // Expand: the tabs' Offstage is zero-sized while hidden, and a loose Stack would shrink to it and lay
      // the onboarding out at width 0.
      body: Stack(
        fit: StackFit.expand,
        children: [
          TickerMode(
            enabled: !firstRun,
            child: Offstage(
              offstage: firstRun,
              child: Stack(
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
                        child: _InboxBadgedTabBar(shell: shell),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          // Opaque: the app's backdrop, so nothing of the tabs underneath shows through.
          if (firstRun)
            const Positioned.fill(child: AtlasBackdrop(child: Onboarding())),
        ],
      ),
    );
  }
}

/// The tab bar with the Inbox count: the one widget that follows the inbox providers.
class _InboxBadgedTabBar extends ConsumerWidget {
  const _InboxBadgedTabBar({required this.shell});
  final StatefulNavigationShell shell;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return FloatingTabBar(
      currentIndex: shell.currentIndex,
      onTabSelected: (i) =>
          shell.goBranch(i, initialLocation: i == shell.currentIndex),
      inboxCount:
          ref.watch(inboxProvider).length +
          unreadCount(
            ref.watch(recentActivityProvider),
            ref.watch(inboxSeenProvider),
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
      if (ref.read(p.exists)) f(ref.read(p.notifier));
    }
  }

  @override
  void initState() {
    super.initState();
    _life;
    // Push is optional: without Firebase config this is a no-op and the token stays null. It never asks for
    // notifications: that happens after the first pairing or in Settings, with an explanation.
    PushService.init(ref);
  }

  @override
  void dispose() {
    _life.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // Registers the FCM token (and its relay grant) with the connected computers.
    ref.watch(pushRegistrationProvider);
    return MaterialApp.router(
      title: 'Atlas',
      debugShowCheckedModeBanner: false,
      theme: atlasTheme(),
      localizationsDelegates: atlasLocalizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      routerConfig: _router,
      // The app's ground under every route; tapping a push opens the agent it is about, over whatever tab is
      // showing.
      builder: (context, child) => atlasGround(
        context,
        PushTapHost(navigatorKey: _rootNavigatorKey, child: child!),
      ),
    );
  }
}
