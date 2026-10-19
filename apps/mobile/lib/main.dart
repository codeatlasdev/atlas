import 'package:flutter/material.dart';

import 'package:atlas_mobile/src/rust/frb_generated.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/pair/pair_screen.dart';
import 'package:atlas_mobile/features/projects/project_detail_screen.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';

final _router = GoRouter(
  routes: [
    GoRoute(path: '/', builder: (_, _) => const HomeScreen()),
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
      theme: atlasTheme(Brightness.light),
      darkTheme: atlasTheme(Brightness.dark),
      themeMode: ThemeMode.system,
      routerConfig: _router,
    );
  }
}
