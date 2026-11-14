import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/features/agents/agent_chat_screen.dart';
import 'package:atlas_mobile/features/agents/agent_launcher.dart';
import 'package:atlas_mobile/features/agents/agent_switcher.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/home/onboarding.dart';
import 'package:atlas_mobile/features/pair/pair_screen.dart';
import 'package:atlas_mobile/features/settings/settings_screen.dart';
import 'package:atlas_mobile/features/projects/project_detail_screen.dart';
import 'package:atlas_mobile/features/terminal/terminal_screen.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/lab/lab.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/markdown/atlas_markdown.dart';

import 'package:atlas_mobile/core/demo/demo_computer.dart';

import 'support/layout_harness.dart';
import 'support/stress_computer.dart';

class _Storage extends MacStorage {
  _Storage(this.devices) : super(storage: null);
  final List<MacDevice> devices;
  @override
  Future<List<MacDevice>> loadDevices() async => devices;
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

/// The phone with [devices] paired, talking to [computer] (the demo, or the stress
/// computer that prints every name as a 60-character string). Above the app, so the
/// sheets and dialogs a screen opens see it too.
AppWrapper _phone({
  List<MacDevice> devices = const [demoDevice],
  Transport Function()? computer,
  List<Map<String, String>> integrations = const [],
}) =>
    (app) => ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage(devices)),
        transportFactoryProvider.overrideWithValue((_, _) async {
          final t =
              computer?.call() ?? demoTransport(integrations: integrations);
          // A computer the app dropped without closing (a connection rebuilt while it
          // was still dialling) keeps its timers: `_check` closes every one before the
          // test ends (a tearDown runs after the timer check, too late).
          _dialled.add(t);
          return t;
        }),
      ],
      child: app,
    );

/// Every computer the app dialled during the current check.
final _dialled = <Transport>[];

Transport _stress() => StressComputer(demoTransport());

const _off = [
  {'id': 'claude', 'label': 'Claude Code', 'state': 'missing'},
  {'id': 'omp', 'label': 'omp', 'state': 'ok'},
];

/// `expectNoLayoutProblems`, with the phone's storage and computer wired.
Future<void> _check(
  WidgetTester t,
  String label,
  Widget Function(BuildContext) screen, {
  Transport Function()? computer,
  List<MacDevice> devices = const [demoDevice],
  Future<void> Function(WidgetTester)? prepare,
  double keyboard = 0,
  bool tapTargets = true,
  int settleMs = 3000,
}) async {
  try {
    await expectNoLayoutProblems(
      t,
      screen,
      wrap: _phone(computer: computer, devices: devices, integrations: _off),
      label: label,
      prepare: prepare,
      keyboard: keyboard,
      tapTargets: tapTargets,
      settleFor: Duration(milliseconds: settleMs),
    );
  } finally {
    // Not awaited: a stream close completes on the real event loop, which a
    // fake-async test never gives it. Its timers are cancelled synchronously.
    for (final c in _dialled) {
      unawaited(c.close());
    }
    _dialled.clear();
    await t.pump();
  }
}

/// Past the splash and the onboarding (debug builds show it on every launch) to the
/// paired computer's dashboard.
Future<void> _openDashboard(WidgetTester t) async {
  for (var i = 0; i < 60 && find.text('Continue').evaluate().isEmpty; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
  for (var i = 0; i < 3; i++) {
    await t.tap(find.text('Continue'));
    await settle(t, 700);
  }
  await t.tap(find.text('Open my computer'));
  await settle(t, 3500);
}

void main() {
  const device = demoDevice;

  group('onboarding', () {
    testWidgets('every page fits', (t) async {
      await _check(
        t,
        'onboarding',
        (_) => const Scaffold(body: Onboarding()),
        settleMs: 300,
        prepare: (t) async {
          for (var i = 0; i < 3; i++) {
            await t.tap(find.text('Continue'));
            await settle(t, 700);
          }
        },
      );
    });
  });

  group('home', () {
    testWidgets('dashboard, demo computer', (t) async {
      await _check(
        t,
        'home',
        (_) => const HomeScreen(),
        prepare: _openDashboard,
        settleMs: 300,
      );
    });

    testWidgets('dashboard, long names everywhere', (t) async {
      await _check(
        t,
        'home (stress)',
        (_) => const HomeScreen(),
        computer: _stress,
        devices: const [stressDevice, demoDevice],
        prepare: _openDashboard,
        settleMs: 300,
      );
    });

    // Settings is a tab of its own now (no sheet behind a header button).
    testWidgets('settings tab, long names', (t) async {
      await _check(
        t,
        'settings',
        (_) => const SettingsScreen(),
        computer: _stress,
        devices: const [stressDevice, demoDevice],
        settleMs: 600,
      );
    });
  });

  group('pair', () {
    testWidgets('error state with a long message', (t) async {
      await _check(
        t,
        'pair error',
        (_) => const PairScreen(),
        tapTargets: false,
        settleMs: 100,
        prepare: (t) async {
          final c = ProviderScope.containerOf(
            t.element(find.byType(PairScreen)),
          );
          unawaited(
            c
                .read(pairNotifierProvider.notifier)
                .pair(
                  'atlas-mobile://pair?node=$longHash&code=1&name=$longHost',
                ),
          );
          await settle(t, 600);
        },
      );
    });
  });

  group('project', () {
    Widget screen(BuildContext _, {MacDevice d = device}) =>
        ProjectDetailScreen(
          device: d,
          project: const ProjectInfo(
            id: 'demo-shop',
            root: longPath,
            name: longName,
            running: true,
            services: [],
          ),
        );

    testWidgets('overview, demo', (t) async {
      await _check(t, 'project', screen);
    });
    testWidgets('overview, long names', (t) async {
      await _check(
        t,
        'project (stress)',
        (c) => screen(c, d: stressDevice),
        computer: _stress,
        devices: const [stressDevice],
      );
    });
    for (final tab in ['Agents', 'Metrics']) {
      testWidgets('$tab tab, long names', (t) async {
        await _check(
          t,
          'project $tab',
          (c) => screen(c, d: stressDevice),
          computer: _stress,
          devices: const [stressDevice],
          prepare: (t) async {
            await scrollToAndTap(t, find.textContaining(tab));
          },
        );
      });
    }
    testWidgets('service sheet and logs, long names', (t) async {
      await _check(
        t,
        'project service sheet',
        (c) => screen(c, d: stressDevice),
        computer: _stress,
        devices: const [stressDevice],
        prepare: (t) async {
          await scrollToAndTap(t, find.textContaining('service-'));
        },
      );
      await _check(
        t,
        'project logs',
        (c) => screen(c, d: stressDevice),
        computer: _stress,
        devices: const [stressDevice],
        prepare: (t) async {
          await scrollToAndTap(t, find.text('Logs'));
          await settle(t, 600);
        },
      );
    });
  });

  group('agent chat', () {
    final status = AgentStatus(
      state: 'done',
      agent: 'omp',
      updatedAt: DateTime.now().millisecondsSinceEpoch,
      stateStartedAt: DateTime.now().millisecondsSinceEpoch,
    );
    Widget chat(BuildContext _, String pane) => AgentChatScreen(
      device: device,
      paneKey: pane,
      initial: status,
      title: longProse,
    );

    for (final pane in ['demo-omp', 'demo-claude']) {
      testWidgets(pane, (t) async {
        await _check(t, 'chat $pane', (c) => chat(c, pane), keyboard: 300);
      });
      testWidgets('$pane, long names', (t) async {
        await _check(
          t,
          'chat $pane (stress)',
          (c) => chat(c, pane),
          computer: _stress,
          keyboard: 300,
        );
      });
    }
  });

  group('agent hub, launcher, switcher', () {
    testWidgets('hub and detail sheet, long names', (t) async {
      Widget hub(BuildContext _) => AgentChatScreen(
        device: device,
        paneKey: 'demo-omp',
        initial: AgentStatus(
          state: 'done',
          agent: 'omp',
          updatedAt: 1,
          stateStartedAt: 1,
        ),
        title: 'x',
      );
      await _check(
        t,
        'hub',
        hub,
        computer: _stress,
        prepare: (t) async {
          await scrollToAndTap(t, find.byTooltip('Agent hub'));
          await scrollToAndTap(t, find.textContaining('LedgerScout'));
        },
      );
    });
    testWidgets('launcher', (t) async {
      await _check(
        t,
        'launcher',
        (c) => Scaffold(
          body: Center(
            child: ElevatedButton(
              onPressed: () =>
                  showAgentLauncher(c, device: device, projectId: 'demo-shop'),
              child: const Text('open'),
            ),
          ),
        ),
        keyboard: 300,
        prepare: (t) async {
          await t.tap(find.text('open'));
          await settle(t, 800);
        },
      );
    });
    testWidgets('switcher, long names', (t) async {
      final all = {
        for (var i = 0; i < 12; i++)
          'p$i': AgentStatus(
            state: i.isEven ? 'blocked' : 'working',
            agent: 'claude',
            title: longProse,
            updatedAt: i,
            stateStartedAt: i,
          ),
      };
      await _check(
        t,
        'switcher',
        (c) => Scaffold(
          body: Center(
            child: ElevatedButton(
              onPressed: () => showAgentSwitcher(c, agents: all, current: 'p0'),
              child: const Text('open'),
            ),
          ),
        ),
        prepare: (t) async {
          await t.tap(find.text('open'));
          await settle(t, 800);
        },
      );
    });
  });

  group('terminal', () {
    Widget term(BuildContext _, String pty, {String? pane}) =>
        TerminalScreen(device: device, pty: pty, title: longProse, pane: pane);

    testWidgets('feed (omp), read-only', (t) async {
      await _check(
        t,
        'terminal feed',
        (c) => term(c, 'pty-demo-omp', pane: 'demo-omp'),
        computer: _stress,
      );
    });
    testWidgets('whole screen (claude)', (t) async {
      await _check(
        t,
        'terminal screen',
        (c) => term(c, 'pty-demo-claude', pane: 'demo-claude'),
      );
    });
  });

  group('states and widgets the flows above do not reach', () {
    testWidgets('terminal key bar, Ctrl row and line input', (t) async {
      await _check(
        t,
        'terminal input',
        (_) => Scaffold(
          body: Column(
            children: [
              const Spacer(),
              KeyBar(onKey: (_) {}, enabled: true),
              LineInput(onSubmit: (_) {}),
            ],
          ),
        ),
        keyboard: 300,
        settleMs: 100,
        prepare: (t) async {
          await t.tap(find.text('Ctrl'));
          await settle(t, 300);
        },
      );
    });

    testWidgets('home: computer unreachable', (t) async {
      await _check(
        t,
        'home offline',
        (_) => const HomeScreen(),
        computer: () => throw StateError('no route to $longHost'),
        devices: const [stressDevice],
        prepare: (t) async {
          for (
            var i = 0;
            i < 60 && find.text('Continue').evaluate().isEmpty;
            i++
          ) {
            await t.pump(const Duration(milliseconds: 100));
          }
          for (var i = 0; i < 3; i++) {
            await t.tap(find.text('Continue'));
            await settle(t, 700);
          }
          await t.tap(find.text('Open my computer'));
          await settle(t, 2500);
        },
        settleMs: 100,
      );
    });

    testWidgets('project: load error and skeleton', (t) async {
      await _check(
        t,
        'project error',
        (_) => const ProjectDetailScreen(
          device: demoDevice,
          project: ProjectInfo(
            id: 'demo-shop',
            root: longPath,
            name: longName,
            running: false,
            services: [
              ServiceInfo(name: longName, state: 'running'),
              ServiceInfo(name: 'b', state: 'failed'),
            ],
          ),
        ),
        computer: () => throw StateError('no route to $longHost'),
        settleMs: 400,
      );
    });

    testWidgets('empty state with a long message and an action', (t) async {
      await _check(
        t,
        'empty state',
        (_) => Scaffold(
          body: EmptyState(
            title: longProse,
            message: '$longProse $longHash $longPath',
            action: FilledButton(
              onPressed: () {},
              child: const Text('Try again'),
            ),
          ),
        ),
        keyboard: 300,
        settleMs: 100,
      );
    });

    testWidgets('lab banner', (t) async {
      await _check(
        t,
        'lab banner',
        (_) => LabFrame(
          lab: const LabConfig(enabled: true, sha: longHash),
          child: const Scaffold(body: Center(child: Text('app'))),
        ),
        settleMs: 100,
      );
    });

    testWidgets('logo', (t) async {
      await _check(
        t,
        'logo',
        (_) => const Scaffold(
          body: Center(child: AtlasLogo(size: 148, animated: true)),
        ),
        settleMs: 2000,
      );
    });

    testWidgets('markdown: long inline code, links, lists, headings, tables', (
      t,
    ) async {
      const md =
          '''
# Heading $longName

Run `$longHash` then `$longPath` and see [$longPath](https://example.com/$longHash).

- item $longProse
  - nested `$longHash`
1. first $longPath

> quote $longProse

| name | value | path |
|---|---|---|
| $longName | $longHash | $longPath |

```bash
echo $longPath $longHash
```
''';
      await _check(
        t,
        'markdown',
        (_) => const Scaffold(
          body: SingleChildScrollView(
            child: Padding(
              padding: EdgeInsets.all(16),
              child: AtlasMarkdown(md),
            ),
          ),
        ),
        settleMs: 100,
      );
    });
  });
}
