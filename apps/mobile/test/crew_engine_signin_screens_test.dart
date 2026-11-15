// The two places a phone signs an engine in: the "Sign in to …" action card of a waiting task (a real task
// from hostd with the real `sign_in_engine` action) and the engine rows of Crew settings. Both start
// `crew_engine_login` on the crew server and open its terminal (`/terminal`, which keeps typing behind Face ID).
// Incident (PhoneCrewAudit gap 2): the card said "Sign in … in Settings" and Settings had nothing to tap.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/action_card.dart';
import 'package:atlas_mobile/features/crew/ops/crew_settings_screen.dart';

import 'support/demo_harness.dart';

dynamic _json(String name) =>
    jsonDecode(File('test/fixtures/$name').readAsStringSync());

/// The real waiting task of hostd, carrying the real `sign_in_engine` action of `engine`.
CrewmateTask _signInTask(String engine) {
  final tasks = (_json('crewmate_task_prove_ui.json') as Map)['tasks'] as List?;
  final base =
      (tasks?.first ?? _json('crewmate_task_prove_ui.json')['task'])
          as Map<String, dynamic>;
  final action = Map<String, dynamic>.from(
    (_json('task_actions.json') as List).firstWhere(
      (a) => a['kind'] == 'sign_in_engine',
    ) as Map,
  )..['engine'] = engine;
  return CrewmateTask.fromJson({
    ...base,
    'state': 'waiting_action',
    'action': action,
  });
}

/// Leaves the pushed terminal page, as the back gesture does.
Future<void> _back(WidgetTester t) async {
  final nav = Navigator.of(t.element(find.text('terminal screen')));
  nav.pop();
  await settle(t);
}

void main() {
  const mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

  /// [home] under a router that records where `/terminal` is pushed, with the demo computer behind it.
  Future<List<Map<String, dynamic>>> pump(WidgetTester t, Widget home) async {
    final opened = <Map<String, dynamic>>[];
    final router = GoRouter(
      routes: [
        GoRoute(
          path: '/',
          // The chat screen and the Crew tab hold the connection while they are up; so does this host.
          builder: (_, _) => Scaffold(
            body: Consumer(
              builder: (_, ref, _) {
                ref.watch(connectionNotifierProvider(mac));
                return home;
              },
            ),
          ),
        ),
        GoRoute(
          path: '/terminal',
          builder: (_, state) {
            opened.add((state.extra! as Map).cast<String, dynamic>());
            return const Scaffold(body: Text('terminal screen'));
          },
        ),
      ],
    );
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(
      ProviderScope(
        overrides: [
          macStorageProvider.overrideWithValue(DemoStorage([mac])),
          transportFactoryProvider.overrideWithValue(
            (_, _) async => demoTransport(),
          ),
        ],
        child: MaterialApp.router(theme: atlasTheme(), routerConfig: router),
      ),
    );
    await settle(t);
    return opened;
  }

  testWidgets('the sign-in card opens the engine\'s own sign-in terminal', (
    t,
  ) async {
    final opened = await pump(
      t,
      TaskActionCard(
        task: _signInTask('codex'),
        device: mac,
        crewmateId: 'crewmate-1',
      ),
    );
    expect(find.text('Sign in to Codex'), findsOneWidget);
    expect(find.textContaining('in Settings'), findsNothing);
    await t.tap(find.byKey(const Key('sign-in-here')));
    await settle(t);
    expect(opened, hasLength(1), reason: 'the terminal of the sign-in opened');
    expect(opened.single['pty'], 'pty-signin-codex');
    expect(opened.single['title'], 'Sign in: Codex');
    await _back(t);
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });

  testWidgets(
    'an engine the phone cannot sign in keeps Done and offers no button',
    (t) async {
      await pump(
        t,
        TaskActionCard(
          task: _signInTask('some-future-engine'),
          device: mac,
          crewmateId: 'crewmate-1',
        ),
      );
      expect(find.byKey(const Key('sign-in-here')), findsNothing);
      expect(find.text('Done'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    },
  );

  testWidgets(
    'Crew settings lists each engine with its state and signs one in',
    (t) async {
      final opened = await pump(t, const CrewSettingsScreen(device: mac));
      expect(find.text('Engine sign-in'), findsOneWidget);
      expect(find.text('Signed in'), findsNWidgets(2)); // claude, omp
      expect(find.text('Needs sign-in'), findsOneWidget); // codex
      expect(
        find.text('Not installed on this server'),
        findsOneWidget,
      ); // opencode
      // Nothing to sign in for an engine that is not installed.
      expect(find.byKey(const Key('engine-signin-opencode')), findsNothing);

      await t.tap(find.byKey(const Key('engine-signin-codex')));
      await settle(t);
      expect(opened.single['pty'], 'pty-signin-codex');

      // Back from the terminal the row asks again: the demo "finished" the sign-in.
      expect(find.text('terminal screen'), findsOneWidget);
      await _back(t);
      expect(find.text('Needs sign-in'), findsNothing);
      expect(find.text('Signed in'), findsNWidgets(3));
      await t.pumpWidget(const SizedBox());
      await settle(t);
    },
  );
}
