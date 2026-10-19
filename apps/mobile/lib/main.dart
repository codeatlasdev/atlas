import 'package:flutter/material.dart';

import 'package:atlas_mobile/src/rust/frb_generated.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

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

class AtlasMobileApp extends StatelessWidget {
  const AtlasMobileApp({super.key});

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
