import 'package:flutter/material.dart';

import 'package:atlas_mobile/src/rust/frb_generated.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/push/push_registration.dart';
import 'package:atlas_mobile/core/push/push_service.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/bots/bot_screen.dart';
import 'package:atlas_mobile/features/bots/bots_screen.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/pair/pair_screen.dart';
import 'package:atlas_mobile/features/projects/project_detail_screen.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';
final _router = GoRouter(
  routes: [
    StatefulShellRoute.indexedStack(
      builder: (context, state, navigationShell) =>
          _AppShell(navigationShell: navigationShell),
      branches: [
        StatefulShellBranch(
          routes: [
            GoRoute(
              path: '/',
              builder: (_, _) => const HomeScreen(),
            ),
          ],
        ),
        StatefulShellBranch(
          routes: [
            GoRoute(
              path: '/bots',
              builder: (_, _) => const BotsScreen(),
              routes: [
                GoRoute(
                  path: ':nodeId/:botId',
                  builder: (_, state) {
                    final args = state.extra! as Map<String, dynamic>;
                    return BotScreen(
                      device: args['device'] as MacDevice,
                      botId: state.pathParameters['botId']!,
                    );
                  },
                ),
              ],
            ),
          ],
        ),
      ],
    ),
    GoRoute(path: '/pair', builder: (_, _) => const PairScreen()),
    GoRoute(
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
  ],
);

class _AppShell extends StatelessWidget {
  const _AppShell({required this.navigationShell});
  final StatefulNavigationShell navigationShell;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: navigationShell,
      bottomNavigationBar: NavigationBar(
        selectedIndex: navigationShell.currentIndex,
        onDestinationSelected: (index) => navigationShell.goBranch(
          index,
          // Re-tap on the current tab returns to its root.
          initialLocation: index == navigationShell.currentIndex,
        ),
        destinations: const [
          NavigationDestination(
            icon: Icon(Icons.computer_rounded),
            label: 'Computers',
          ),
          NavigationDestination(
            icon: Icon(Icons.smart_toy_rounded),
            label: 'Crew',
          ),
        ],
      ),
    );
  }
}

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
    _life; // initialise eagerly
    PushService.init(null); // best effort; token arrives via fcmTokenProvider
    // Push: on every token refresh, re-register with all paired bot computers.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        ref.listen<String?>(fcmTokenProvider, (_, token) {
          if (token != null) _registerPushToken(token);
        });
        // Also register the initial token if already available.
        final token = ref.read(fcmTokenProvider);
        if (token != null) _registerPushToken(token);
      }
    });
  }

  @override
  void dispose() {
    _life.dispose();
    super.dispose();
  }

  Future<void> _registerPushToken(String token) async {
    final devices = ref.read(pairedDevicesProvider).value ?? const [];
    final secretKey = await ref.read(macStorageProvider).loadSecretKey() ?? '';
    for (final device in devices) {
      try {
        final transport = await ref.read(transportFactoryProvider)(device, secretKey);
        final mux = Multiplexer(transport);
        try {
          if (await checkBotsCap(mux)) {
            await PushService.registerWithComputer(mux, token);
          }
        } finally {
          await mux.close();
        }
      } catch (_) {
        // Best effort.
      }
    }
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
